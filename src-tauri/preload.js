
// Preload script to bridge Slack notifications to Tauri

// 1. MOCK Service Workers
if (window.navigator) {
    const dummyServiceWorker = {
        controller: null,
        ready: new Promise(() => {}), // Never resolves
        getRegistration: () => Promise.resolve(undefined),
        register: () => Promise.reject(new Error("ServiceWorkers disabled in Zlack")),
        addEventListener: () => {},
        removeEventListener: () => {},
        dispatchEvent: () => false,
    };

    Object.defineProperty(window.navigator, 'serviceWorker', {
        get: function() {
            return dummyServiceWorker;
        },
        configurable: true
    });
}

// 2. Mock Permission API
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


// 2.5 Intercept Network Requests (Telemetry) for Notification Context
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

        if (url.pathname === '/app_redirect') {
            return preventSlackNativeRedirect(url);
        }
    } catch (error) {
        console.error('Zlack: Failed to normalize Slack internal URL', error);
    }

    return href;
}

// 5. Intercept External Links
document.addEventListener('click', (e) => {
    const target = e.target.closest('a');
    if (target && target.href) {
        // Check if it's an external link (http/https) and NOT part of the Slack app itself
        const originalHref = target.href;
        const href = normalizeSlackInternalUrl(originalHref);
        const isExternal = href.startsWith('http') && 
                           !href.includes('app.slack.com') && 
                           !href.includes('slack.com');

        const opensInNewTab = target.target === '_blank';
        const isNormalizedInternalSlackLink = href !== originalHref;

        if (isExternal) {
            console.log("Zlack: Intercepted external link click:", href);
            e.preventDefault();
            e.stopPropagation();
            const openExternal = window.__TAURI__?.shell?.open;
            if (typeof openExternal === 'function') {
                openExternal(href).catch((error) => {
                    console.error('Zlack: Failed to open external link via Tauri', error);
                    window.open(href, '_blank');
                });
            } else {
                window.open(href, '_blank');
            }
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
