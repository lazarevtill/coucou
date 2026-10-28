//! Read a selected browser's navigation bar on an explicit attachment click.
//! Never focus it, press keys, use the clipboard, or walk page documents.
use windows::Win32::{Foundation::HWND, System::Com::{CoCreateInstance, CoInitializeEx,
    CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED}, UI::Accessibility::*};
use std::time::{Duration, Instant};

fn web_address(text: &str) -> Option<String> {
    let text = text.trim();
    if text.len() > 8192 || text.chars().any(char::is_control) { return None; }
    let mut url = reqwest::Url::parse(text).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() { return None; }
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    Some(url.into())
}

fn supported_browser(executable: &str) -> bool {
    matches!(executable.to_ascii_lowercase().as_str(), "chrome" | "msedge" | "firefox" | "brave" | "vivaldi" | "opera")
}

pub(super) fn browser_url(hwnd: HWND, executable: &str) -> Option<String> {
    if !supported_browser(executable) { return None; }
    struct Com(bool);
    impl Drop for Com { fn drop(&mut self) { if self.0 { unsafe { CoUninitialize(); } } } }
    unsafe {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let automation: IUIAutomation2 = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()?;
        automation.SetConnectionTimeout(350).ok()?;
        automation.SetTransactionTimeout(350).ok()?;
        let root = automation.ElementFromHandle(hwnd).ok()?;
        let walker = automation.ControlViewWalker().ok()?;
        let started = Instant::now();
        let mut pending = vec![(root, false, 0usize)];
        let mut visited = 0;
        let mut addresses = std::collections::BTreeSet::new();
        while let Some((node, toolbar, depth)) = pending.pop() {
            if visited >= 100 || started.elapsed() > Duration::from_secs(2) { break; }
            visited += 1;
            let Ok(kind) = node.CurrentControlType() else { continue };
            // A web page can imitate an address field. Its document subtree is
            // excluded, even when the page labels an element as a toolbar.
            if kind == UIA_DocumentControlTypeId || node.CurrentIsPassword().is_ok_and(|v| v.as_bool()) { continue; }
            let toolbar = toolbar || kind == UIA_ToolBarControlTypeId;
            if toolbar && (kind == UIA_EditControlTypeId || kind == UIA_ComboBoxControlTypeId)
                && node.CurrentIsPassword().is_ok_and(|v| !v.as_bool())
                && node.CurrentIsOffscreen().is_ok_and(|v| !v.as_bool()) {
                if let Ok(pattern) = node.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                    if let Ok(value) = pattern.CurrentValue() {
                        if let Some(url) = web_address(&value.to_string()) { addresses.insert(url); }
                    }
                }
            }
            if depth >= 8 { continue; }
            let mut child = walker.GetFirstChildElement(&node).ok();
            while let Some(element) = child {
                if pending.len() + visited >= 100 || started.elapsed() > Duration::from_secs(2) { break; }
                child = walker.GetNextSiblingElement(&element).ok();
                pending.push((element, toolbar, depth + 1));
            }
        }
        // Ambiguous navigation/search UI never becomes a guessed attachment.
        (addresses.len() == 1).then(|| addresses.into_iter().next().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_addresses_are_absolute_web_urls_without_embedded_credentials() {
        assert_eq!(web_address("https://user:password@example.com/path?q=1#anchor").as_deref(), Some("https://example.com/path?q=1#anchor"));
        assert_eq!(web_address("http://localhost:3000/").as_deref(), Some("http://localhost:3000/"));
        for value in ["file:///private.txt", "javascript:alert(1)", "data:text/plain,private", "example.com", "search these words", "https://example.com/\nprivate"] {
            assert!(web_address(value).is_none(), "{value}");
        }
        assert!(web_address(&format!("https://example.com/{}", "a".repeat(8192))).is_none());
        assert!(supported_browser("Chrome"));
        assert!(!supported_browser("password-manager"));
    }
}
