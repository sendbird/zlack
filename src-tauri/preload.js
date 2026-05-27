
// Preload script to bridge Slack notifications to Tauri

// 1. Mock Permission API
const originalQuery = navigator.permissions.query;
navigator.permissions.query = (parameters) => {
    if (parameters.name === 'notifications') {
        return Promise.resolve({
            state: 'granted',
            addEventListener: () => {},
            removeEventListener: () => {},
            onchange: null
        });
    }
    return originalQuery.call(navigator.permissions, parameters);
};


// 2. Intercept Network Requests (Telemetry) for Notification Context
let lastEventContext = { teamId: 'unknown', channelId: 'unknown' };
window.__zlackLastEventContext = lastEventContext;


function processTelemetryBody(bodyStr) {
    try {
        if (!bodyStr || !bodyStr.includes('notification:sent')) return;
        
        const data = JSON.parse(bodyStr);
        if (!data.spans) return;

        data.spans.forEach(span => {
            if (span.name === 'notification:sent' && span.tags) {
                // Extracted tags
                let tid = null;
                let cid = null;
                
                span.tags.forEach(tag => {
                    if (tag.key === 'encoded_team_id') tid = tag.v_str;
                    if (tag.key === 'encoded_channel_id') cid = tag.v_str;
                });

                if (tid && cid) {
                    console.log(`[Zlack] Captured context from network: Team=${tid}, Channel=${cid}`);
                    lastEventContext.teamId = tid;
                    lastEventContext.channelId = cid;
                }
            }
        });
    } catch (e) {
        // Ignore JSON parse errors or other issues
    }
}

// Intercept fetch
const originalFetch = window.fetch;
window.fetch = function(input, init) {
    if (init && init.body) {
        // Clone body is tricky with streams, but Slack's telemetry is usually string/json
        if (typeof init.body === 'string') {
            processTelemetryBody(init.body);
        }
    }
    return originalFetch.apply(this, arguments);
};

// Intercept navigator.sendBeacon
const originalSendBeacon = navigator.sendBeacon;
navigator.sendBeacon = function(url, data) {
    if (data && typeof data === 'string') {
        processTelemetryBody(data);
    }
    return originalSendBeacon.apply(this, arguments);
};

// Intercept XMLHttpRequest (just in case they use it for some calls)
const originalXHRScan = XMLHttpRequest.prototype.send;
XMLHttpRequest.prototype.send = function(body) {
    if (body && typeof body === 'string') {
        processTelemetryBody(body);
    }
    return originalXHRScan.apply(this, arguments);
};


// 3. Shim Notification API
const ZlackNotification = class {
  constructor(title, options, ...args) {
    this.title = title;
    this.options = options || {};
    this.clickHandlers = [];
    
    // Store as global pending notification
    window.__ZlackPendingNotification = this;

    try {
      if (window.__TAURI__) {
          // Delay slightly to ensure network/console logs have updated context
          setTimeout(() => {
              const teamId = lastEventContext.teamId || 'unknown';
              const channelId = lastEventContext.channelId || 'unknown';
              const originalBody = this.options.body || '';

              (window.__TAURI__.core?.invoke || window.__TAURI__.invoke)('notify', {
                title: typeof title === 'string' ? title : 'New Message',
                body: originalBody,
                teamId: teamId,
                channelId: channelId
              });
          }, 500); // Wait for network telemetry to be captured
      }
    } catch (e) {
      console.error('Zlack: Failed to invoke notify', e);
    }
  }

  static get permission() { return "granted"; }
  static requestPermission(cb) {
    if (cb) cb("granted");
    return Promise.resolve("granted");
  }

  // Support addEventListener for 'click' (dummy mostly, as we rely on native focus)
  addEventListener(type, listener) {
    if (type === 'click' && typeof listener === 'function') {
        this.clickHandlers.push(listener);
    }
  }
  
  removeEventListener(type, listener) {
    if (type === 'click') {
        this.clickHandlers.filter(l => l !== listener);
    }
  }

  close() {}
};

// Force the shim to stay
Object.defineProperty(window, 'Notification', {
    value: ZlackNotification,
    writable: false,
    configurable: false
});




// 4. Add drag regions for macOS overlay titlebar
function installZlackDragRegions() {
    const STRIP_ID = 'zlack-drag-strip';
    const HANDLE_ID = 'zlack-drag-handle';
    const RESTORE_HANDLE_ID = 'zlack-restore-drag-handle';
    const TRAFFIC_LIGHT_OFFSET = 80;
    const STRIP_HEIGHT = 28;
    const DRAG_START_THRESHOLD = 4;
    const INTERACTIVE_TOP_BAR_SELECTOR = [
        'input',
        'textarea',
        'select',
        'button',
        'a[href]',
        '[contenteditable]:not([contenteditable="false"])',
        '[role="button"]',
        '[role="link"]',
        '[role="searchbox"]',
        '[role="textbox"]',
        '[role="menuitem"]',
        '[role="menuitemcheckbox"]',
        '[role="menuitemradio"]',
        '[role="tab"]',
        '[role="combobox"]',
        '[role="switch"]',
        '[role="checkbox"]',
        '[role="radio"]',
        '[tabindex]:not([tabindex="-1"])',
    ].join(', ');
    const dragRegionState = window.__zlackDragRegionState || (window.__zlackDragRegionState = {});

    const getCurrentTauriWindow = () => {
        try {
            const getCurrentWindow = window.__TAURI__?.window?.getCurrentWindow;
            if (typeof getCurrentWindow !== 'function') {
                return null;
            }

            return getCurrentWindow();
        } catch (error) {
            console.error('Zlack: Failed to resolve current Tauri window', error);
            return null;
        }
    };

    const invokeTauriCommand = (command) => {
        try {
            const invoke = window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke;
            if (typeof invoke !== 'function') {
                throw new Error(`Tauri invoke is unavailable for command: ${command}`);
            }

            return invoke(command);
        } catch (error) {
            console.error(`Zlack: Failed to invoke Tauri command ${command}`, error);
            throw error;
        }
    };

    const startWindowDrag = async () => {
        const appWindow = getCurrentTauriWindow();
        if (appWindow && typeof appWindow.startDragging === 'function') {
            return appWindow.startDragging();
        }

        return invokeTauriCommand('start_window_drag');
    };

    const toggleWindowMaximize = async () => {
        try {
            return await invokeTauriCommand('toggle_window_maximize');
        } catch (error) {
            const appWindow = getCurrentTauriWindow();
            if (appWindow && typeof appWindow.toggleMaximize === 'function') {
                return appWindow.toggleMaximize();
            }

            throw error;
        }
    };

    const detachPendingDragListeners = () => {
        if (!dragRegionState.pendingDragListenersAttached) {
            return;
        }

        window.removeEventListener('mousemove', handlePendingDragMove, true);
        window.removeEventListener('mouseup', clearPendingDrag, true);
        window.removeEventListener('blur', clearPendingDrag);
        document.removeEventListener('mouseleave', clearPendingDrag, true);
        dragRegionState.pendingDragListenersAttached = false;
    };

    const clearPendingDrag = () => {
        dragRegionState.pendingDrag = null;
        detachPendingDragListeners();
    };

    const attachPendingDragListeners = () => {
        if (dragRegionState.pendingDragListenersAttached) {
            return;
        }

        window.addEventListener('mousemove', handlePendingDragMove, true);
        window.addEventListener('mouseup', clearPendingDrag, true);
        window.addEventListener('blur', clearPendingDrag);
        document.addEventListener('mouseleave', clearPendingDrag, true);
        dragRegionState.pendingDragListenersAttached = true;
    };

    const getEventElementTarget = (target) => {
        if (target instanceof Element) {
            return target;
        }

        if (target instanceof Node) {
            return target.parentElement;
        }

        return null;
    };

    const isInteractiveTopBarTarget = (target) => {
        const element = getEventElementTarget(target);
        if (!element) {
            return false;
        }

        return Boolean(element.closest(INTERACTIVE_TOP_BAR_SELECTOR));
    };

    const isTopBarChromeEvent = (event) => {
        if (event.button !== 0) {
            return false;
        }

        if (event.clientY > STRIP_HEIGHT || event.clientX < TRAFFIC_LIGHT_OFFSET) {
            return false;
        }

        return !isInteractiveTopBarTarget(event.target);
    };

    const handlePendingDragMove = async (event) => {
        const pendingDrag = dragRegionState.pendingDrag;
        if (!pendingDrag) {
            return;
        }

        if ((event.buttons & 1) !== 1) {
            clearPendingDrag();
            return;
        }

        const deltaX = event.clientX - pendingDrag.startX;
        const deltaY = event.clientY - pendingDrag.startY;
        if (Math.hypot(deltaX, deltaY) < DRAG_START_THRESHOLD) {
            return;
        }

        clearPendingDrag();
        event.preventDefault();
        event.stopPropagation();

        try {
            await startWindowDrag();
        } catch (error) {
            console.error('Zlack: Failed to handle manual drag region action', error);
        }
    };

    const handleTopBarMouseDown = (event) => {
        if (!isTopBarChromeEvent(event)) {
            return;
        }

        event.preventDefault();
        event.stopPropagation();
        clearPendingDrag();
        dragRegionState.pendingDrag = {
            startX: event.clientX,
            startY: event.clientY,
        };
        attachPendingDragListeners();
    };

    const handleTopBarDoubleClick = async (event) => {
        if (!isTopBarChromeEvent(event)) {
            return;
        }

        clearPendingDrag();
        event.preventDefault();
        event.stopPropagation();

        try {
            await toggleWindowMaximize();
        } catch (error) {
            console.error('Zlack: Failed to handle drag region double click', error);
        }
    };

    const ensureDragStrip = () => {
        let strip = document.getElementById(STRIP_ID);
        if (!strip) {
            strip = document.createElement('div');
            strip.id = STRIP_ID;
        }

        Object.assign(strip.style, {
            position: 'fixed',
            top: '0px',
            left: '0px',
            right: '0px',
            height: `${STRIP_HEIGHT}px`,
            zIndex: '2147483647',
            background: 'transparent',
            pointerEvents: 'none',
            userSelect: 'none',
            WebkitUserSelect: 'none',
        });
        strip.style.width = `${window.innerWidth}px`;

        document.getElementById(HANDLE_ID)?.remove();
        document.getElementById(RESTORE_HANDLE_ID)?.remove();

        if (strip.parentElement !== document.documentElement) {
            document.documentElement.appendChild(strip);
        }

        return strip;
    };

    const requestEnsureDragStrip = () => {
        if (dragRegionState.ensureDragStripPending) {
            return;
        }

        dragRegionState.ensureDragStripPending = true;
        dragRegionState.ensureDragStripFrameId = window.requestAnimationFrame(() => {
            dragRegionState.ensureDragStripPending = false;
            dragRegionState.ensureDragStripFrameId = null;
            ensureDragStrip();
        });
    };

    const registerTauriResizeListener = () => {
        if (dragRegionState.tauriResizeListenerRegistered || dragRegionState.tauriResizeListenerRegistering) {
            return;
        }

        const appWindow = getCurrentTauriWindow();
        if (!appWindow) {
            return;
        }

        dragRegionState.tauriResizeListenerRegistering = true;

        if (typeof appWindow.onResized === 'function') {
            Promise.resolve(appWindow.onResized(() => {
                requestEnsureDragStrip();
            }))
                .then((unlisten) => {
                    dragRegionState.tauriResizeListenerRegistered = true;
                    dragRegionState.tauriResizeUnlisten = typeof unlisten === 'function' ? unlisten : null;
                })
                .catch((error) => {
                    console.error('Zlack: Failed to register Tauri resize listener', error);
                })
                .finally(() => {
                    dragRegionState.tauriResizeListenerRegistering = false;
                });
            return;
        }

        if (typeof appWindow.listen === 'function') {
            Promise.resolve(appWindow.listen('tauri://resize', () => {
                requestEnsureDragStrip();
            }))
                .then((unlisten) => {
                    dragRegionState.tauriResizeListenerRegistered = true;
                    dragRegionState.tauriResizeUnlisten = typeof unlisten === 'function' ? unlisten : null;
                })
                .catch((error) => {
                    console.error('Zlack: Failed to register legacy Tauri resize listener', error);
                })
                .finally(() => {
                    dragRegionState.tauriResizeListenerRegistering = false;
                });
            return;
        }

        dragRegionState.tauriResizeListenerRegistering = false;
    };

    dragRegionState.ensureDragStrip = ensureDragStrip;
    dragRegionState.requestEnsureDragStrip = requestEnsureDragStrip;

    if (!dragRegionState.documentTopBarListenersInstalled) {
        document.addEventListener('mousedown', handleTopBarMouseDown, true);
        document.addEventListener('dblclick', handleTopBarDoubleClick, true);
        document.addEventListener('mouseup', clearPendingDrag, true);
        dragRegionState.documentTopBarListenersInstalled = true;
    }

    if (!dragRegionState.windowResizeHandlerInstalled) {
        window.addEventListener('resize', () => {
            requestEnsureDragStrip();
            registerTauriResizeListener();
        });
        dragRegionState.windowResizeHandlerInstalled = true;
    }

    if (!dragRegionState.mutationObserver) {
        dragRegionState.mutationObserver = new MutationObserver(() => {
            requestEnsureDragStrip();
        });
        dragRegionState.mutationObserver.observe(document.documentElement, { childList: true, subtree: true });
    }

    ensureDragStrip();
    registerTauriResizeListener();
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', installZlackDragRegions, { once: true });
} else {
    installZlackDragRegions();
}

function preventSlackNativeRedirect(url) {
    url.searchParams.set('no_native_redirect', '1');
    return url.toString();
}

function normalizeSlackPermalink(url) {
    const archiveMatch = url.pathname.match(/^\/archives\/([^/]+)\/(p\d{16})$/);
    if (!archiveMatch) {
        return null;
    }

    const [, channelId, messageId] = archiveMatch;
    const normalizedUrl = new URL(url.toString());
    normalizedUrl.pathname = '/messages/' + channelId + '/' + messageId;
    normalizedUrl.searchParams.delete('cid');
    return preventSlackNativeRedirect(normalizedUrl);
}

function normalizeSlackInternalUrl(href) {
    try {
        const url = new URL(href, window.location.href);
        const isSlackHost = url.hostname === 'slack.com' || url.hostname.endsWith('.slack.com');
        if (!isSlackHost || url.hostname === 'app.slack.com') {
            return href;
        }

        const permalinkUrl = normalizeSlackPermalink(url);
        if (permalinkUrl) {
            return permalinkUrl;
        }

        if (url.pathname === '/app_redirect' || url.pathname.startsWith('/files/')) {
            return preventSlackNativeRedirect(url);
        }
    } catch (error) {
        console.error('Zlack: Failed to normalize Slack internal URL', error);
    }

    return href;
}

function isSlackHost(hostname) {
    return hostname === 'slack.com' || hostname.endsWith('.slack.com');
}

function isSlackFileHost(hostname) {
    return hostname === 'slack-files.com'
        || hostname.endsWith('.slack-files.com')
        || hostname === 'slack-edge.com'
        || hostname.endsWith('.slack-edge.com');
}

function isSlackFileUrl(url) {
    if (!url || (url.protocol !== 'http:' && url.protocol !== 'https:')) {
        return false;
    }

    if (isSlackFileHost(url.hostname)) {
        return true;
    }

    return isSlackHost(url.hostname)
        && (url.pathname.startsWith('/files/')
            || url.pathname.startsWith('/files-pri/')
            || url.pathname.startsWith('/files-tmb/'));
}

function isSlackFilePermalinkUrl(url) {
    if (!url || !isSlackHost(url.hostname)) {
        return false;
    }

    const pathname = url.pathname || '';
    return pathname.startsWith('/files/')
        && !pathname.includes('/download/')
        && url.searchParams.get('download') !== '1';
}

function isDirectSlackDownloadUrl(url) {
    if (!isSlackFileUrl(url) || isSlackFilePermalinkUrl(url)) {
        return false;
    }

    const pathname = url.pathname || '';
    return isSlackFileHost(url.hostname)
        || pathname.startsWith('/files-pri/')
        || pathname.includes('/download/')
        || url.searchParams.get('download') === '1';
}

const ZOOM_EXTERNAL_PROTOCOLS = new Set(['zoommtg:', 'zoomus:', 'zoomphonecall:']);
const originalWindowOpen = window.open.bind(window);

function parseUrl(href) {
    try {
        return new URL(String(href), window.location.href);
    } catch (error) {
        console.error('Zlack: Failed to parse URL', error);
        return null;
    }
}

function isZoomProtocolUrl(url) {
    return Boolean(url && ZOOM_EXTERNAL_PROTOCOLS.has(url.protocol));
}

function isZoomHost(hostname) {
    return hostname === 'zoom.us' || hostname.endsWith('.zoom.us') || hostname === 'zoom.com' || hostname.endsWith('.zoom.com');
}

function isExternalHttpUrl(url) {
    return Boolean((url.protocol === 'http:' || url.protocol === 'https:')
        && !isSlackHost(url.hostname)
        && !isSlackFileHost(url.hostname));
}

function isZoomExternalUrl(url) {
    return isZoomProtocolUrl(url) || ((url.protocol === 'http:' || url.protocol === 'https:') && isZoomHost(url.hostname));
}

function shouldOpenOutsideSlack(href) {
    const url = parseUrl(href);
    return Boolean(url && (isZoomExternalUrl(url) || isExternalHttpUrl(url)));
}

async function openExternalLink(href) {
    const invoke = window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.__TAURI_INTERNALS__?.invoke;
    if (typeof invoke === 'function') {
        return invoke('open_external_url', { url: href });
    }

    const openExternal = window.__TAURI__?.shell?.open;
    if (typeof openExternal === 'function') {
        return openExternal(href);
    }

    originalWindowOpen(href, '_blank', 'noopener,noreferrer');
}

function startWebviewDownload(href) {
    const frameName = 'zlack-download-frame';
    let frame = document.getElementById(frameName);
    if (!frame) {
        frame = document.createElement('iframe');
        frame.id = frameName;
        frame.name = frameName;
        frame.title = 'Zlack download target';
        frame.style.display = 'none';
        document.documentElement.appendChild(frame);
    }

    const link = document.createElement('a');
    link.href = href;
    link.download = '';
    link.target = frameName;
    link.rel = 'noopener';
    link.style.display = 'none';
    document.documentElement.appendChild(link);
    link.click();
    link.remove();

    window.setTimeout(() => {
        try {
            frame.src = 'about:blank';
        } catch (_) {}
    }, 1500);
}

function handleWindowOpenUrl(href, fallbackTarget = '_blank', fallbackFeatures = 'noopener,noreferrer') {
    const url = parseUrl(href);
    if (!url) {
        return false;
    }

    if (isSlackFileUrl(url)) {
        const downloadUrl = normalizeSlackInternalUrl(url.toString());
        console.log('Zlack: Starting Slack file download in current webview:', downloadUrl);
        startWebviewDownload(downloadUrl);
        return true;
    }

    if (shouldOpenOutsideSlack(url.toString())) {
        const externalUrl = url.toString();
        console.log('Zlack: Intercepted external window.open:', externalUrl);
        openExternalLink(externalUrl).catch((error) => {
            console.error('Zlack: Failed to open external window.open via Tauri', error);
            originalWindowOpen(externalUrl, fallbackTarget, fallbackFeatures);
        });
        return true;
    }

    return false;
}

function createDeferredWindowOpenProxy(target, features) {
    let href = 'about:blank';
    let closed = false;
    const fallbackTarget = target || '_blank';
    const fallbackFeatures = features || 'noopener,noreferrer';

    const openAssignedUrl = (value) => {
        href = String(value || 'about:blank');
        if (href === 'about:blank') {
            return;
        }

        if (!handleWindowOpenUrl(href, fallbackTarget, fallbackFeatures)) {
            originalWindowOpen(href, fallbackTarget, fallbackFeatures);
        }
    };

    const locationProxy = {
        assign: openAssignedUrl,
        replace: openAssignedUrl,
        toString: () => href,
    };

    Object.defineProperty(locationProxy, 'href', {
        get: () => href,
        set: openAssignedUrl,
        configurable: true,
    });

    return {
        get closed() {
            return closed;
        },
        close() {
            closed = true;
        },
        focus() {},
        blur() {},
        document: {
            write() {},
            close() {},
        },
        get location() {
            return locationProxy;
        },
        set location(value) {
            openAssignedUrl(value);
        },
    };
}

window.open = function(href, target, features) {
    if (!href || String(href) === 'about:blank') {
        return createDeferredWindowOpenProxy(target, features);
    }

    if (handleWindowOpenUrl(href, target, features)) {
        return null;
    }

    return originalWindowOpen(href, target, features);
};

function getElementTarget(target) {
    if (target instanceof Element) {
        return target;
    }

    if (target instanceof Node) {
        return target.parentElement;
    }

    return null;
}

function getNearbyZoomCardText(element) {
    let current = element;
    for (let depth = 0; current && depth < 8; depth += 1) {
        const text = current.innerText || current.textContent || '';
        if (text.length < 5000 && /(?:Zoom meeting|Meeting ID)/i.test(text)) {
            return text;
        }
        current = current.parentElement;
    }

    return '';
}

function buildZoomJoinUrl(cardText) {
    const meetingMatch = cardText.match(/Meeting\s+ID\s*:?\s*([0-9][0-9\s-]{6,}[0-9])/i);
    if (!meetingMatch) {
        return null;
    }

    const confno = meetingMatch[1].replace(/\D/g, '');
    if (confno.length < 9) {
        return null;
    }

    const passcodeMatch = cardText.match(/(?:Meeting\s+)?passcode\s*:?\s*([\s\S]*?)(?=To receive|\n|$)/i);
    const passcode = passcodeMatch?.[1]?.trim().replace(/[.,;:]+$/, '');
    const zoomUrl = new URL('zoommtg://zoom.us/join');
    zoomUrl.searchParams.set('action', 'join');
    zoomUrl.searchParams.set('confno', confno);
    if (passcode) {
        zoomUrl.searchParams.set('pwd', passcode);
    }

    return zoomUrl.toString();
}

function getClickableControl(target) {
    const element = getElementTarget(target);
    return element?.closest('a[href], button, [role="button"], [tabindex]:not([tabindex="-1"])') || null;
}

function getControlDescriptor(element) {
    if (!element) {
        return '';
    }

    return [
        element.textContent,
        element.getAttribute('aria-label'),
        element.getAttribute('title'),
        element.getAttribute('data-qa'),
        element.getAttribute('data-qa-tooltip'),
        element.getAttribute('data-testid'),
        element.getAttribute('aria-keyshortcuts'),
    ].filter(Boolean).join(' ');
}

function isExplicitDownloadControl(element) {
    const descriptor = getControlDescriptor(element);
    return /\bdownload\b|다운로드|다운 받|다운받|저장/i.test(descriptor);
}

function isVisibleControl(element) {
    if (!element) return false;
    const rect = element.getBoundingClientRect();
    if (rect.width < 12 || rect.height < 12) return false;
    const style = window.getComputedStyle(element);
    return style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity || '1') > 0.01;
}

function getBoundedSlackFileCardContext(element) {
    let current = element;

    for (let depth = 0; current && depth < 8; depth += 1) {
        if (current === document.body || current === document.documentElement) {
            return null;
        }

        const rect = current.getBoundingClientRect?.();
        if (rect && (rect.width > Math.min(window.innerWidth * 0.75, 900) || rect.height > 360)) {
            return null;
        }

        const links = Array.from(current.querySelectorAll?.('a[href]') || []);
        const hasSlackFileLink = links.some((link) => isSlackFileUrl(parseUrl(link.href)));
        if (hasSlackFileLink) {
            const text = (current.innerText || current.textContent || '').slice(0, 4000);
            if (/\.[a-z0-9]{2,8}(?:\b|$)/i.test(text)) {
                return current;
            }
        }

        current = current.parentElement;
    }

    return null;
}

function isLeftmostFileActionControl(element, boundaryElement) {
    const rect = element.getBoundingClientRect();
    let current = element.parentElement;

    for (let depth = 0; current && depth < 7; depth += 1) {
        if (boundaryElement && !boundaryElement.contains(current)) {
            return false;
        }

        const controls = Array.from(current.querySelectorAll('a[href], button, [role="button"], [tabindex]:not([tabindex="-1"])'))
            .filter(isVisibleControl)
            .map((control) => ({ control, rect: control.getBoundingClientRect() }))
            .filter(({ rect: candidateRect }) => Math.abs((candidateRect.top + candidateRect.bottom) / 2 - (rect.top + rect.bottom) / 2) < 12);

        if (controls.length >= 2) {
            controls.sort((a, b) => a.rect.left - b.rect.left);
            return controls[0].control === element;
        }

        if (current === boundaryElement) {
            return false;
        }

        current = current.parentElement;
    }

    return false;
}

function getSlackFileUrlPriority(url) {
    if (!isSlackFileUrl(url)) {
        return -1;
    }

    const pathname = (url.pathname || '').toLowerCase();
    const decodedPathname = (() => {
        try {
            return decodeURIComponent(pathname);
        } catch (_) {
            return pathname;
        }
    })();
    const isPdf = /\.pdf(?:$|[/?#])/i.test(decodedPathname);

    if (isDirectSlackDownloadUrl(url) && isPdf) {
        return 80;
    }

    if (isDirectSlackDownloadUrl(url)) {
        return 75;
    }

    if (isSlackFileHost(url.hostname) && isPdf) {
        return 70;
    }

    if (isSlackHost(url.hostname) && pathname.startsWith('/files-pri/')) {
        return 65;
    }

    if (isSlackFileHost(url.hostname)) {
        return 60;
    }

    if (isSlackHost(url.hostname) && pathname.startsWith('/files-tmb/')) {
        return 10;
    }

    if (isSlackFilePermalinkUrl(url)) {
        return 5;
    }

    return 20;
}

function findSlackFileUrlNear(element, options = {}) {
    const seen = new Set();
    let bestCandidate = null;
    let current = element;

    for (let depth = 0; current && depth < 10; depth += 1) {
        const candidates = [];
        if (current.matches?.('a[href]')) {
            candidates.push(current);
        }
        candidates.push(...Array.from(current.querySelectorAll?.('a[href]') || []));

        for (const link of candidates) {
            if (seen.has(link)) {
                continue;
            }
            seen.add(link);

            const url = parseUrl(link.href);
            if (options.directOnly && !isDirectSlackDownloadUrl(url)) {
                continue;
            }

            const priority = getSlackFileUrlPriority(url);
            if (priority >= 0 && (!bestCandidate || priority > bestCandidate.priority)) {
                bestCandidate = {
                    priority,
                    url: normalizeSlackInternalUrl(url.toString()),
                };
                if (priority >= 50) {
                    return bestCandidate.url;
                }
            }
        }

        current = current.parentElement;
    }

    return bestCandidate?.url || null;
}

function getFilenameFromUrl(href) {
    try {
        const url = new URL(href, window.location.href);
        const lastSegment = decodeURIComponent(url.pathname.split('/').filter(Boolean).pop() || 'file');
        return lastSegment || 'file';
    } catch (_) {
        return 'file';
    }
}

function getFileKindLabel(filename) {
    const ext = String(filename || '').split('.').pop()?.toLowerCase();
    const labels = {
        pdf: 'PDF',
        png: 'PNG image',
        jpg: 'JPEG image',
        jpeg: 'JPEG image',
        gif: 'GIF image',
        webp: 'WebP image',
        mov: 'Video',
        mp4: 'Video',
        txt: 'Text',
        md: 'Markdown',
        json: 'JSON',
        csv: 'CSV',
        zip: 'Archive',
    };
    return labels[ext] || (ext ? ext.toUpperCase() : 'File');
}

function getFileIconLabel(filename) {
    const ext = String(filename || '').split('.').pop()?.toLowerCase();
    if (!ext) return '01';
    if (ext === 'pdf') return 'PDF';
    if (['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(ext)) return 'IMG';
    if (['zip', 'gz', 'tar', 'rar', '7z'].includes(ext)) return 'ZIP';
    return ext.slice(0, 3).toUpperCase();
}

async function openZlackDownloadsFolder() {
    const fallbackUrl = 'zlack://open-downloads?source=toast&ts=' + Date.now();
    try {
        window.location.href = fallbackUrl;
    } catch (error) {
        console.error('Zlack: Failed to request Downloads folder via navigation', error);
    }

    const invoke = window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.__TAURI_INTERNALS__?.invoke;
    let lastError = null;
    if (typeof invoke === 'function') {
        try {
            await invoke('open_downloads_folder', {});
            return;
        } catch (error) {
            lastError = error;
            console.error('Zlack: Failed to open Downloads folder via command', error);
        }
    }

    const openExternal = window.__TAURI__?.shell?.open;
    const downloadDir = await window.__TAURI__?.path?.downloadDir?.().catch((error) => {
        lastError = error;
        console.error('Zlack: Failed to resolve Downloads folder', error);
        return null;
    });

    if (typeof openExternal === 'function' && downloadDir) {
        try {
            await openExternal(downloadDir);
            return;
        } catch (error) {
            lastError = error;
        }
    }

    console.error('Zlack: No available path opened Downloads folder', lastError);
}

function showZlackDownloadToast(input) {
    const payload = typeof input === 'object' && input !== null ? input : { filename: String(input || 'file') };
    const filename = payload.filename || getFilenameFromUrl(payload.url || '') || 'file';
    const status = payload.status || 'downloading';
    const success = payload.success !== false;
    const complete = status === 'finished';
    const kind = payload.kind || getFileKindLabel(filename);
    const iconLabel = getFileIconLabel(filename);

    const existing = document.getElementById('zlack-download-toast');
    existing?.remove();

    const toast = document.createElement('div');
    toast.id = 'zlack-download-toast';
    toast.setAttribute('role', 'status');
    toast.setAttribute('aria-live', 'polite');

    const panel = document.createElement('div');
    panel.className = 'zlack-download-toast-panel';

    const card = document.createElement('div');
    card.className = 'zlack-download-toast-card';

    const icon = document.createElement('div');
    icon.className = 'zlack-download-toast-icon';
    icon.textContent = iconLabel;
    if (iconLabel === 'PDF') icon.classList.add('is-pdf');

    const badge = document.createElement('div');
    badge.className = 'zlack-download-toast-badge';
    badge.textContent = complete ? (success ? '✓' : '!') : '↓';
    if (!success) badge.classList.add('is-error');
    icon.appendChild(badge);

    const copy = document.createElement('div');
    copy.className = 'zlack-download-toast-copy';

    const title = document.createElement('div');
    title.className = 'zlack-download-toast-title';
    title.textContent = filename;

    const subtitle = document.createElement('div');
    subtitle.className = 'zlack-download-toast-subtitle';
    subtitle.textContent = complete ? (success ? kind : 'Download failed') : kind;

    copy.append(title, subtitle);
    card.append(icon, copy);

    const downloads = document.createElement('button');
    downloads.type = 'button';
    downloads.className = 'zlack-download-toast-link';
    downloads.textContent = 'View all downloads';
    downloads.addEventListener('click', (event) => {
        event.preventDefault();
        event.stopImmediatePropagation();
        openZlackDownloadsFolder();
    });

    panel.append(card, downloads);
    toast.append(panel);

    const style = document.createElement('style');
    style.textContent = [
        '#zlack-download-toast {',
        '  position: fixed;',
        '  right: 28px;',
        '  bottom: 84px;',
        '  width: min(380px, calc(100vw - 56px));',
        '  z-index: 2147483647;',
        '  pointer-events: none;',
        '  color: #f8f8f8;',
        '  font-family: Slack-Lato, Slack-Averta, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;',
        '  animation: zlackDownloadToastIn 180ms cubic-bezier(.2, .8, .2, 1);',
        '}',
        '#zlack-download-toast .zlack-download-toast-panel {',
        '  box-sizing: border-box;',
        '  width: 100%;',
        '  padding: 12px 14px 14px;',
        '  border: 1px solid rgba(232, 232, 232, .14);',
        '  border-radius: 13px;',
        '  background: rgba(35, 38, 42, .76);',
        '  box-shadow: 0 14px 36px rgba(0, 0, 0, .32), inset 0 1px 0 rgba(255, 255, 255, .045);',
        '  backdrop-filter: blur(18px) saturate(1.12);',
        '  -webkit-backdrop-filter: blur(18px) saturate(1.12);',
        '}',
        '#zlack-download-toast .zlack-download-toast-card {',
        '  display: grid;',
        '  grid-template-columns: 42px minmax(0, 1fr);',
        '  align-items: center;',
        '  gap: 12px;',
        '  min-height: 56px;',
        '  width: 100%;',
        '  padding: 8px 12px;',
        '  border-radius: 10px;',
        '  background: rgba(28, 24, 25, .88);',
        '  box-shadow: inset 0 1px 0 rgba(255, 255, 255, .025);',
        '}',
        '#zlack-download-toast .zlack-download-toast-icon {',
        '  position: relative;',
        '  display: grid;',
        '  place-items: center;',
        '  width: 42px;',
        '  height: 42px;',
        '  border-radius: 9px;',
        '  background: linear-gradient(145deg, #6c6a6d, #4e4c50);',
        '  color: #fff;',
        '  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;',
        '  font-size: 13px;',
        '  font-weight: 800;',
        '  letter-spacing: -.08em;',
        '  overflow: visible;',
        '}',
        '#zlack-download-toast .zlack-download-toast-icon.is-pdf {',
        '  background: linear-gradient(145deg, #ff2d6f, #df1457);',
        '  font-size: 11px;',
        '  letter-spacing: -.04em;',
        '}',
        '#zlack-download-toast .zlack-download-toast-badge {',
        '  position: absolute;',
        '  right: -5px;',
        '  bottom: -5px;',
        '  display: grid;',
        '  place-items: center;',
        '  width: 19px;',
        '  height: 19px;',
        '  border-radius: 999px;',
        '  background: #f8f8f8;',
        '  color: #1d1c1d;',
        '  border: 2px solid rgba(28, 24, 25, .95);',
        '  font-size: 12px;',
        '  font-weight: 900;',
        '  line-height: 1;',
        '}',
        '#zlack-download-toast .zlack-download-toast-badge.is-error { color: #e01e5a; }',
        '#zlack-download-toast .zlack-download-toast-copy { min-width: 0; }',
        '#zlack-download-toast .zlack-download-toast-title {',
        '  overflow: hidden;',
        '  text-overflow: ellipsis;',
        '  white-space: nowrap;',
        '  font-size: 14px;',
        '  line-height: 18px;',
        '  font-weight: 700;',
        '  letter-spacing: -.02em;',
        '  color: #fff;',
        '}',
        '#zlack-download-toast .zlack-download-toast-subtitle {',
        '  margin-top: 3px;',
        '  overflow: hidden;',
        '  text-overflow: ellipsis;',
        '  white-space: nowrap;',
        '  font-size: 13px;',
        '  line-height: 17px;',
        '  font-weight: 500;',
        '  color: rgba(255, 255, 255, .88);',
        '}',
        '#zlack-download-toast .zlack-download-toast-link {',
        '  pointer-events: auto;',
        '  margin: 9px 0 0 54px;',
        '  padding: 0;',
        '  border: 0;',
        '  background: transparent;',
        '  color: rgba(255, 255, 255, .94);',
        '  font: inherit;',
        '  font-size: 13px;',
        '  line-height: 18px;',
        '  font-weight: 500;',
        '  text-align: left;',
        '  cursor: pointer;',
        '}',
        '#zlack-download-toast .zlack-download-toast-link:hover { text-decoration: underline; }',
        '@keyframes zlackDownloadToastIn {',
        '  from { opacity: 0; transform: translateY(14px) scale(.985); }',
        '  to { opacity: 1; transform: translateY(0) scale(1); }',
        '}',
    ].join('\n');
    toast.appendChild(style);

    document.documentElement.appendChild(toast);
    window.clearTimeout(window.__zlackDownloadToastTimeout);
    window.__zlackDownloadToastTimeout = window.setTimeout(() => toast.remove(), complete ? 7000 : 5000);
}

window.__zlackShowDownloadToast = showZlackDownloadToast;

function triggerSlackFileDownload(downloadUrl, event, sourceLabel) {
    if (!downloadUrl) {
        return false;
    }

    const now = Date.now();
    if (window.__zlackLastDownloadUrl === downloadUrl && now - (window.__zlackLastDownloadAt || 0) < 1200) {
        event?.preventDefault();
        event?.stopImmediatePropagation();
        return true;
    }
    window.__zlackLastDownloadUrl = downloadUrl;
    window.__zlackLastDownloadAt = now;

    console.log(`Zlack: Intercepted Slack file ${sourceLabel}:`, downloadUrl);
    event?.preventDefault();
    event?.stopImmediatePropagation();
    startWebviewDownload(downloadUrl);
    return true;
}

function maybeHandleSlackFileDownloadButtonClick(event) {
    const target = getElementTarget(event.target);
    if (target?.closest('#zlack-download-toast')) {
        return false;
    }

    const control = getClickableControl(event.target);
    if (!control) {
        return false;
    }

    const fileCardContext = getBoundedSlackFileCardContext(control);
    const shouldHandle = isExplicitDownloadControl(control)
        || (fileCardContext && isLeftmostFileActionControl(control, fileCardContext));
    if (!shouldHandle) {
        return false;
    }

    const downloadUrl = findSlackFileUrlNear(fileCardContext || control);
    if (!downloadUrl) {
        console.warn('Zlack: Slack download control clicked, but no nearby file URL was found; letting Slack handle it', control);
        return false;
    }

    return triggerSlackFileDownload(downloadUrl, event, 'download control');
}

function maybeHandleZoomJoinButtonClick(event) {
    const target = getElementTarget(event.target);
    if (!target || target.closest('a[href]')) {
        return false;
    }

    const button = target.closest('button, [role="button"]');
    if (!button) {
        return false;
    }

    const buttonText = (button.innerText || button.textContent || button.getAttribute('aria-label') || button.getAttribute('title') || '').trim();
    if (!/(?:^|\b)(join|참여|입장)(?:\b|$)/i.test(buttonText)) {
        return false;
    }

    const cardText = getNearbyZoomCardText(button);
    if (!cardText) {
        return false;
    }

    const zoomUrl = buildZoomJoinUrl(cardText);
    if (!zoomUrl) {
        return false;
    }

    console.log('Zlack: Intercepted Zoom Join button:', zoomUrl);
    event.preventDefault();
    event.stopImmediatePropagation();
    openExternalLink(zoomUrl).catch((error) => {
        console.error('Zlack: Failed to open Zoom Join URL via Tauri', error);
        originalWindowOpen(zoomUrl, '_blank', 'noopener,noreferrer');
    });
    return true;
}

// 5. Intercept External Links
document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) {
        return;
    }

    if (maybeHandleSlackFileDownloadButtonClick(e)) {
        return;
    }
}, true);

document.addEventListener('click', (e) => {
    if (maybeHandleSlackFileDownloadButtonClick(e)) {
        return;
    }

    if (maybeHandleZoomJoinButtonClick(e)) {
        return;
    }

    const eventTarget = getElementTarget(e.target);
    const target = eventTarget?.closest('a');
    if (target && target.href) {
        // Check if it's an external http(s) link and NOT part of Slack itself.
        const originalHref = target.href;
        const href = normalizeSlackInternalUrl(originalHref);
        const isExternal = shouldOpenOutsideSlack(href);

        const opensInNewTab = target.target === '_blank';
        const isNormalizedInternalSlackLink = href !== originalHref;

        if (isExternal) {
            console.log("Zlack: Intercepted external link click:", href);
            e.preventDefault();
            e.stopImmediatePropagation();
            openExternalLink(href).catch((error) => {
                console.error('Zlack: Failed to open external link via Tauri', error);
                originalWindowOpen(href, '_blank', 'noopener,noreferrer');
            });
        } else if (opensInNewTab || isNormalizedInternalSlackLink) {
            // Internal Slack link meant for a new tab: keep it in this webview,
            // but do not manually assign window.location. Let Slack/default navigation
            // honor any redirect chain all the way to the final web route.
            console.log("Zlack: Keeping internal Slack link in current webview:", href);
            if (href !== originalHref) {
                target.href = href;
            }
            target.target = '_self';
        }
    }
}, true); // Capture phase to ensure we get it before Slack
