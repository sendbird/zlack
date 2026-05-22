// 6. Native Slack Desktop shortcut actions
function getZlackCurrentTeamId() {
    const pathMatch = window.location.pathname.match(/\/client\/([^/]+)/);
    if (pathMatch) return pathMatch[1];
    const eventContext = window.__zlackLastEventContext || {};
    if (eventContext.teamId && eventContext.teamId !== 'unknown') return eventContext.teamId;

    const slackGlobals = [
        window.TS?.boot_data?.team_id,
        window.TS?.model?.team?.id,
        window.TS?.model?.team_id,
    ];
    const globalTeamId = slackGlobals.find((teamId) => typeof teamId === 'string' && teamId.startsWith('T'));
    if (globalTeamId) return globalTeamId;

    for (const storage of [window.localStorage, window.sessionStorage]) {
        try {
            for (let index = 0; index < storage.length; index += 1) {
                const key = storage.key(index);
                const value = key ? storage.getItem(key) : null;
                const match = value && value.match(/\bT[A-Z0-9]{8,}\b/);
                if (match) return match[0];
            }
        } catch (error) {
            console.error('Zlack: Failed to scan Slack storage for team id', error);
        }
    }

    return null;
}

function getZlackCurrentChannelId() {
    const match = window.location.pathname.match(/\/client\/[^/]+\/([^/?#]+)/);
    return match ? match[1] : null;
}

function isZlackVisibleElement(element) {
    if (!element) return false;
    const rect = element.getBoundingClientRect();
    const style = window.getComputedStyle(element);
    return rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
}

function describeZlackElement(element) {
    if (!element) return 'none';
    return [
        element.tagName && element.tagName.toLowerCase(),
        element.getAttribute('data-qa') && 'data-qa=' + element.getAttribute('data-qa'),
        element.getAttribute('aria-label') && 'aria=' + element.getAttribute('aria-label'),
        element.id && 'id=' + element.id,
        element.className && typeof element.className === 'string' && 'class=' + element.className.slice(0, 80),
    ].filter(Boolean).join(' ');
}

function setZlackShortcutDetail(detail) {
    window.__zlackShortcutActionDetail = detail;
}

function getZlackElementText(element) {
    return [
        element?.getAttribute?.('aria-label'),
        element?.getAttribute?.('title'),
        element?.getAttribute?.('data-qa'),
        element?.getAttribute?.('data-sk'),
        element?.id,
        typeof element?.className === 'string' ? element.className : '',
        element?.textContent,
    ].filter(Boolean).join(' ');
}

function getZlackClickableElement(element, options = {}) {
    if (!element) return null;

    const {
        preferDescendantSelectors = [],
    } = options;

    const directMatch = element.closest('button, [role="button"], a[href], input, textarea, [tabindex]:not([tabindex="-1"])');
    if (directMatch) return directMatch;

    for (const selector of preferDescendantSelectors) {
        const descendant = element.querySelector(selector);
        if (descendant && isZlackVisibleElement(descendant)) return descendant;
    }

    return element.querySelector('a[href], button, [role="button"], input, textarea, [tabindex]:not([tabindex="-1"])') || element;
}

function activateZlackElement(element, detail, options = {}) {
    const clickable = getZlackClickableElement(element, options);
    if (!clickable || !isZlackVisibleElement(clickable)) {
        setZlackShortcutDetail(detail + ' -> no visible clickable ancestor from ' + describeZlackElement(element));
        return false;
    }

    clickable.focus?.({ preventScroll: true });
    clickable.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, view: window }));
    clickable.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, cancelable: true, view: window }));
    clickable.click();
    setZlackShortcutDetail(detail + ' -> clicked ' + describeZlackElement(clickable));
    return true;
}

function clickZlackFirstVisible(selectors) {
    for (const selector of selectors) {
        const elements = Array.from(document.querySelectorAll(selector));
        const element = elements.find(isZlackVisibleElement);
        if (element && activateZlackElement(element, 'selector ' + selector + ' matches=' + elements.length)) {
            return true;
        }
    }
    setZlackShortcutDetail('no selector matched: ' + selectors.join(', '));
    return false;
}

function clickZlackButtonByText(patterns) {
    const controls = Array.from(document.querySelectorAll('button, [role="button"], a, [role="menuitem"]'));
    const control = controls.find((element) => {
        if (!isZlackVisibleElement(element)) return false;
        const text = getZlackElementText(element);
        return patterns.some((pattern) => pattern.test(text));
    });
    if (!control) {
        setZlackShortcutDetail('no text button matched: ' + patterns.map(String).join(', '));
        return false;
    }
    return activateZlackElement(control, 'text match ' + patterns.map(String).join(', '));
}

function navigateZlackSlackRoute(route, options = {}) {
    const { allowHardNavigation = true } = options;
    const teamId = getZlackCurrentTeamId();
    if (!teamId) {
        console.warn('Zlack: Cannot navigate Slack shortcut without team id:', route);
        setZlackShortcutDetail('route ' + route + ' -> no team id');
        return false;
    }

    const routePath = '/client/' + teamId + '/' + route;
    const links = Array.from(document.querySelectorAll('a[href]'));
    const link = links.find((element) => {
        if (!isZlackVisibleElement(element)) return false;
        try {
            const href = new URL(element.href, window.location.href);
            return href.pathname === routePath || href.pathname.endsWith('/' + route);
        } catch (_) {
            return false;
        }
    });

    if (link) {
        return activateZlackElement(link, 'route ' + route + ' via existing link');
    }

    if (!allowHardNavigation) {
        setZlackShortcutDetail('route ' + route + ' -> no visible link and hard navigation disabled');
        return false;
    }

    const url = 'https://app.slack.com' + routePath;
    setZlackShortcutDetail('route ' + route + ' -> hard navigation ' + url);
    window.location.assign(url);
    return true;
}

function clickZlackRegionControl(patterns, options = {}) {
    const {
        selectors = 'button, [role="button"], a, [role="tab"], [role="menuitem"], [tabindex]:not([tabindex="-1"])',
        leftMax = Number.POSITIVE_INFINITY,
        leftMin = Number.NEGATIVE_INFINITY,
        topMin = Number.NEGATIVE_INFINITY,
        topMax = Number.POSITIVE_INFINITY,
        hrefPatterns = [],
        requireHref = false,
    } = options;

    const candidates = Array.from(document.querySelectorAll(selectors))
        .filter(isZlackVisibleElement)
        .map((element) => {
            const rect = element.getBoundingClientRect();
            const text = getZlackElementText(element);
            const href = typeof element.href === 'string' ? element.href : '';
            return { element, rect, text, href };
        })
        .filter(({ rect, text, href }) => {
            if (rect.left < leftMin || rect.left > leftMax) return false;
            if (rect.top < topMin || rect.top > topMax) return false;
            if (requireHref && !href) return false;
            return patterns.some((pattern) => pattern.test(text))
                || hrefPatterns.some((pattern) => pattern.test(href));
        })
        .sort((left, right) => {
            const score = ({ element, rect, text, href }) => {
                let total = 0;
                if (element.tagName === 'A') total += 4;
                if (rect.left <= 320) total += 3;
                if (hrefPatterns.some((pattern) => pattern.test(href))) total += 8;
                if (patterns.some((pattern) => pattern.test(text))) total += 6;
                if (rect.top <= 180) total += 2;
                total -= Math.min(rect.left, 500) / 1000;
                total -= Math.min(rect.top, 1000) / 1000;
                return total;
            };
            return score(right) - score(left);
        });

    const candidate = candidates[0];
    if (!candidate) {
        setZlackShortcutDetail('no regional control matched: ' + patterns.map(String).join(', '));
        return false;
    }

    return activateZlackElement(
        candidate.element,
        'regional match ' + patterns.map(String).join(', ') + ' -> ' + candidate.text.slice(0, 120),
    );
}

function normalizeZlackShortcutText(text) {
    return String(text || '')
        .replace(/\s+/g, ' ')
        .trim()
        .toLowerCase();
}

function clickZlackLeftNavDestination(labels, options = {}) {
    const {
        hrefPatterns = [],
        leftMax = 440,
        topMin = 70,
        topMax = Number.POSITIVE_INFINITY,
        requireHrefMatch = false,
    } = options;

    const normalizedLabels = labels.map(normalizeZlackShortcutText);
    const candidates = Array.from(document.querySelectorAll(
        'a[href], button, [role="button"], [role="tab"], [role="treeitem"], [role="menuitem"], [data-qa], [aria-label]',
    ))
        .filter(isZlackVisibleElement)
        .map((element) => {
            const rect = element.getBoundingClientRect();
            const text = getZlackElementText(element);
            const normalizedText = normalizeZlackShortcutText(text);
            const actionable = getZlackClickableElement(element, {
                preferDescendantSelectors: [
                    ...hrefPatterns.map(() => 'a[href]'),
                    'button',
                    '[role="button"]',
                    '[role="tab"]',
                    '[tabindex]:not([tabindex="-1"])',
                ],
            });
            const href = typeof actionable?.href === 'string'
                ? actionable.href
                : (typeof element.href === 'string' ? element.href : '');
            const qa = normalizeZlackShortcutText(element.getAttribute('data-qa'));
            const aria = normalizeZlackShortcutText(element.getAttribute('aria-label'));
            return { element, actionable, rect, text, normalizedText, href, qa, aria };
        })
        .filter(({ rect, normalizedText, href, qa, aria }) => {
            if (rect.left > leftMax || rect.top < topMin || rect.top > topMax) return false;
            const textMatches = normalizedLabels.some((label) =>
                normalizedText === label
                || normalizedText.startsWith(label + ' ')
                || normalizedText.includes(' ' + label + ' ')
            );
            const attrMatches = normalizedLabels.some((label) => qa.includes(label) || aria.includes(label));
            const hrefMatches = hrefPatterns.some((pattern) => pattern.test(href));
            if (requireHrefMatch && !hrefMatches) return false;
            return textMatches || attrMatches || hrefMatches;
        })
        .sort((left, right) => {
            const score = ({ element, actionable, rect, normalizedText, href, qa, aria }) => {
                let total = 0;
                if (element.tagName === 'A') total += 4;
                if (actionable?.tagName === 'A') total += 12;
                if (rect.left <= 320) total += 3;
                if (normalizedLabels.some((label) => normalizedText === label)) total += 20;
                if (normalizedLabels.some((label) => normalizedText.startsWith(label + ' '))) total += 10;
                if (normalizedLabels.some((label) => qa.includes(label) || aria.includes(label))) total += 8;
                if (hrefPatterns.some((pattern) => pattern.test(href))) total += 30;
                total -= rect.top / 1000;
                total -= rect.left / 1000;
                return total;
            };
            return score(right) - score(left);
        });

    const candidate = candidates[0];
    if (!candidate) {
        setZlackShortcutDetail('no left-nav destination matched: ' + labels.join(', '));
        return false;
    }

    return activateZlackElement(
        candidate.actionable || candidate.element,
        'left-nav match ' + labels.join(', ') + ' -> ' + candidate.text.slice(0, 120) + ' href=' + candidate.href,
        {
            preferDescendantSelectors: [
                'a[href]',
                'button',
                '[role="button"]',
                '[role="tab"]',
                '[tabindex]:not([tabindex="-1"])',
            ],
        },
    );
}

function openZlackSlackNewMessage() {
    return clickZlackFirstVisible([
        '[aria-label*="new message" i]',
        '[aria-label*="compose" i]',
        '[data-qa*="new_message" i]',
        '[data-qa*="compose" i]',
        'button[title*="New message" i]',
        'button[title*="Compose" i]',
    ]) || clickZlackRegionControl(
        [/new message/i, /\bcompose\b/i, /새 메시지/, /\b작성\b/],
        { leftMax: 380, topMax: 160 },
    ) || navigateZlackSlackRoute('compose', { allowHardNavigation: false });
}

function openZlackSlackNewCanvas() {
    return clickZlackControlByAttribute([/new canvas/i, /canvas/i, /새 캔버스/, /캔버스/])
        || navigateZlackSlackRoute('canvas');
}

function openZlackSlackSearch() {
    const opened = clickZlackFirstVisible([
        'button[data-qa="top_nav_search"]',
        '[data-qa="top_nav_search"] button',
        '[data-qa="top_nav_search_button"]',
        '[data-qa="top_nav_search_input"]',
        '[data-qa="search_input"]',
        'button[aria-label*="Search"]',
        '[role="button"][aria-label*="Search"]',
        'button[aria-label*="검색"]',
        '[role="button"][aria-label*="검색"]',
        '[placeholder*="Search"]',
        '[placeholder*="검색"]',
    ]) || clickZlackButtonByText([/search/i, /검색/]);
    if (opened) {
        setZlackShortcutDetail((window.__zlackShortcutActionDetail || '') + '\nsearch opened; not auto-submitting');
    }
    return opened;
}

function clickZlackControlByAttribute(patterns) {
    const controls = Array.from(document.querySelectorAll('button, [role="button"], a, [role="tab"], [role="menuitem"], [tabindex]:not([tabindex="-1"])'));
    const control = controls.find((element) => {
        if (!isZlackVisibleElement(element)) return false;
        const text = getZlackElementText(element);
        return patterns.some((pattern) => pattern.test(text));
    });
    if (!control) {
        setZlackShortcutDetail('no attribute control matched: ' + patterns.map(String).join(', '));
        return false;
    }
    return activateZlackElement(control, 'attribute match ' + patterns.map(String).join(', '));
}

function openZlackSlackActivity() {
    return clickZlackControlByAttribute([/activity/i, /notification/i, /mentions/i, /알림/, /활동/, /멘션/])
        || navigateZlackSlackRoute('activity');
}

function openZlackSlackThreads() {
    return clickZlackLeftNavDestination(
        ['Threads', '스레드'],
        {
            leftMax: 440,
            topMin: 70,
            hrefPatterns: [/\/threads(?:[/?#]|$)/i],
            requireHrefMatch: true,
        },
    ) || clickZlackRegionControl(
        [/^threads$/i, /스레드/],
        {
            selectors: 'a[href], button, [role="tab"], [role="button"], [role="treeitem"], [data-qa], [aria-label]',
            leftMax: 440,
            topMin: 70,
            hrefPatterns: [/\/threads(?:[/?#]|$)/i],
        },
    ) || navigateZlackSlackRoute('threads', { allowHardNavigation: false });
}

function openZlackSlackAllUnreads() {
    return clickZlackLeftNavDestination(
        ['All Unreads', 'Unreads', '읽지 않음', '안 읽음'],
        {
            leftMax: 440,
            topMin: 70,
            hrefPatterns: [/all-unreads/i, /\/unreads(?:[/?#]|$)/i],
        },
    ) || clickZlackRegionControl(
        [/all unreads/i, /\bunreads\b/i, /읽지 않음/, /안 읽음/],
        {
            selectors: 'a[href], button, [role="tab"], [role="button"]',
            leftMax: 440,
            topMin: 70,
            hrefPatterns: [/all-unreads/i, /\/unreads(?:[/?#]|$)/i],
        },
    ) || navigateZlackSlackRoute('all-unreads', { allowHardNavigation: false });
}

function openZlackSlackPeople() {
    return clickZlackRegionControl(
        [/people/i, /user groups?/i, /directories/i, /directory/i, /사람/, /사용자 그룹/, /디렉토리/],
        {
            selectors: 'a[href], button, [role="tab"], [role="button"], [role="menuitem"]',
            leftMax: 420,
            topMin: 70,
            hrefPatterns: [/people/i, /directory/i, /directories/i, /user_groups/i],
        },
    ) || navigateZlackSlackRoute('people', { allowHardNavigation: false });
}

function openZlackSlackConversationDetails() {
    if (clickZlackFirstVisible([
        '[data-qa="channel_header_info_button"]',
        '[data-qa="channel_header_channel_name"] button',
        '[data-qa="channel_header_channel_name"]',
        '[data-qa="channel_header"] button[aria-label]',
        '[data-qa="channel_name_button"]',
        '[data-qa="conversation_header"] button[aria-label]',
        '[aria-label*="Open channel details" i]',
        '[aria-label*="View channel details" i]',
        '[aria-label*="Conversation details" i]',
        '[aria-label*="Channel details" i]',
        '[aria-label*="details" i]',
        '[aria-label*="정보"]',
    ])) {
        return true;
    }

    const headerButtons = Array.from(document.querySelectorAll('button, [role="button"]'))
        .filter(isZlackVisibleElement)
        .filter((element) => {
            const rect = element.getBoundingClientRect();
            const text = [element.getAttribute('aria-label'), element.textContent].filter(Boolean).join(' ');
            return rect.top >= 28 && rect.top <= 120 && rect.left > 220 && !/search|검색/i.test(text);
        });

    const candidate = headerButtons.find((element) => {
        const text = [element.getAttribute('aria-label'), element.textContent].filter(Boolean).join(' ');
        return /#|channel|conversation|member|canvas|정보|채널|대화/i.test(text);
    }) || headerButtons[0];

    if (candidate) {
        return activateZlackElement(candidate, 'header details heuristic candidates=' + headerButtons.length);
    }

    return clickZlackButtonByText([/details/i, /정보/]);
}

function openZlackSlackPreferences() {
    if (clickZlackButtonByText([/preferences/i, /환경설정/, /설정/])) return true;
    const openedProfile = clickZlackFirstVisible([
        '[data-qa="user-button"]',
        '[data-qa="user_menu_button"]',
        '[aria-label*="profile" i]',
        '[aria-label*="프로필"]',
    ]) || clickZlackButtonByText([/profile/i, /프로필/]);
    if (!openedProfile) return false;
    window.setTimeout(() => clickZlackButtonByText([/preferences/i, /환경설정/, /설정/]), 100);
    return true;
}

function uploadZlackSlackFile() {
    return clickZlackFirstVisible([
        '[data-qa="message_input_file_button"]',
        '[data-qa="file_upload_button"]',
        '[aria-label*="Attach" i]',
        '[aria-label*="Upload" i]',
        '[aria-label*="첨부"]',
        '[aria-label*="업로드"]',
    ]) || clickZlackButtonByText([/attach/i, /upload/i, /첨부/, /업로드/]);
}

function toggleZlackSlackLeftSidebar() {
    return clickZlackFirstVisible([
        '[data-qa="left_sidebar_toggle"]',
        '[aria-label*="sidebar" i]',
        '[aria-label*="사이드바"]',
    ]) || clickZlackButtonByText([/sidebar/i, /사이드바/]);
}

function moveZlackSidebarSelection(direction, unreadOnly) {
    const teamId = getZlackCurrentTeamId();
    if (!teamId) return false;
    const links = Array.from(document.querySelectorAll('a[href*="/client/' + teamId + '/"]'))
        .filter(isZlackVisibleElement)
        .filter((link) => !/\/thread\//.test(link.href));
    if (links.length === 0) return false;

    const currentChannelId = getZlackCurrentChannelId();
    let candidates = links;
    if (unreadOnly) {
        candidates = links.filter((link) => {
            const text = [link.getAttribute('aria-label'), link.textContent].filter(Boolean).join(' ');
            return /unread|mentions|읽지|멘션/i.test(text) || link.querySelector('[data-qa*="unread"], [class*="unread"], [class*="bold"]');
        });
        if (candidates.length === 0) candidates = links;
    }

    let index = candidates.findIndex((link) => currentChannelId && link.href.includes('/' + currentChannelId));
    if (index === -1) index = direction > 0 ? -1 : 0;
    const nextIndex = (index + direction + candidates.length) % candidates.length;
    candidates[nextIndex].click();
    return true;
}

function runZlackSlackShortcutAction(shortcutId) {
    console.log('Zlack: Running Slack menu action:', shortcutId);
    switch (shortcutId) {
        case 'zlack_file_new_message':
            return openZlackSlackNewMessage();
        case 'zlack_file_new_canvas':
            return openZlackSlackNewCanvas();
        case 'zlack_go_search':
            return openZlackSlackSearch();
        case 'zlack_go_all_unreads':
            return openZlackSlackAllUnreads();
        case 'zlack_go_threads':
            return openZlackSlackThreads();
        case 'zlack_go_all_dms':
            return navigateZlackSlackRoute('all-dms');
        case 'zlack_go_activity':
            return openZlackSlackActivity();
        case 'zlack_go_channel_browser':
            return navigateZlackSlackRoute('browse-channels');
        case 'zlack_go_people':
            return openZlackSlackPeople();
        case 'zlack_go_downloads':
            return navigateZlackSlackRoute('downloads');
        case 'zlack_go_history_back':
            window.history.back();
            return true;
        case 'zlack_go_history_forward':
            window.history.forward();
            return true;
        default:
            console.warn('Zlack: Unknown Slack menu action:', shortcutId);
            return false;
    }
}
function handleZlackPhysicalShortcutFallback(event) {
    if (!event.metaKey || event.altKey || event.ctrlKey) return;
    if (event.key !== '[' && event.key !== ']') return;

    event.preventDefault();
    event.stopPropagation();
    setZlackShortcutDetail('physical key fallback ' + event.key);
    if (event.key === '[') {
        window.history.back();
        showZlackPhysicalShortcutOverlay('zlack_go_history_back', 'physical fallback history.back()');
    } else {
        window.history.forward();
        showZlackPhysicalShortcutOverlay('zlack_go_history_forward', 'physical fallback history.forward()');
    }
}

function showZlackPhysicalShortcutOverlay(shortcutId, detail) {
    const id = 'zlack-shortcut-diagnostic';
    let overlay = document.getElementById(id);
    if (!overlay) {
        overlay = document.createElement('div');
        overlay.id = id;
        overlay.style.cssText = [
            'position: fixed',
            'z-index: 2147483647',
            'top: 44px',
            'right: 16px',
            'max-width: 520px',
            'padding: 12px 14px',
            'border-radius: 10px',
            'background: rgba(20, 20, 24, 0.94)',
            'color: #fff',
            'font: 12px/1.4 -apple-system, BlinkMacSystemFont, sans-serif',
            'box-shadow: 0 8px 28px rgba(0, 0, 0, 0.36)',
            'white-space: pre-wrap',
            'pointer-events: none'
        ].join(';');
        document.documentElement.appendChild(overlay);
    }
    overlay.textContent = 'Zlack shortcut\n' + shortcutId + '\naction result: true\n' + detail + '\n' + window.location.href;
    window.clearTimeout(window.__zlackShortcutDiagnosticTimer);
    window.__zlackShortcutDiagnosticTimer = window.setTimeout(() => overlay.remove(), 5000);
}

if (!window.__zlackPhysicalShortcutFallbackInstalled) {
    window.addEventListener('keydown', handleZlackPhysicalShortcutFallback, true);
    document.addEventListener('keydown', handleZlackPhysicalShortcutFallback, true);
    window.__zlackPhysicalShortcutFallbackInstalled = true;
}
