//! Resolve a call application's actual endpoints, independently of the Windows
//! defaults. Core Audio sessions are inspected only while audio is requested.
use super::audio::Source;

#[derive(Clone, Debug, Default)]
pub struct Route {
    pub microphone: Option<String>,
    pub playback: Option<String>,
    pub application: Option<String>,
    pub warning: Option<String>,
}

#[derive(Clone, Debug)]
struct Session {
    family: u32,
    app: String,
    input: bool,
    endpoint: String,
}

pub fn resolve(source: &Source) -> (Source, Option<String>, Option<String>) {
    if matches!(source, Source::Mic(Some(_)) | Source::System(Some(_))) { return (source.clone(), None, None); }
    let route = current();
    let endpoint = if matches!(source, Source::Mic(_)) { route.microphone } else { route.playback };
    let app = endpoint.as_ref().and(route.application);
    let selected = match source { Source::Mic(_) => Source::Mic(endpoint), Source::System(_) => Source::System(endpoint) };
    (selected, app, route.warning)
}

fn choose(sessions: &[Session], previous: Option<u32>) -> (Route, Option<u32>) {
    let families: std::collections::BTreeSet<_> = sessions.iter().filter(|s| s.input).map(|s| s.family).collect();
    let family = match families.len() {
        0 => previous.filter(|id| sessions.iter().any(|s| s.family == *id)),
        1 => families.first().copied(),
        _ => return (Route { warning: Some(crate::i18n::t("Multiple apps are using microphones. Choose your call devices in Settings.")), ..Default::default() }, None),
    };
    let Some(family) = family else { return (Route::default(), None) };
    let members: Vec<_> = sessions.iter().filter(|s| s.family == family).collect();
    let endpoint = |input| {
        let ids: std::collections::BTreeSet<_> = members.iter().filter(|s| s.input == input).map(|s| s.endpoint.clone()).collect();
        (ids.len() == 1).then(|| ids.first().unwrap().clone())
    };
    let microphone = endpoint(true);
    let playback = endpoint(false);
    let ambiguous = [true, false].into_iter().any(|input| {
        members.iter().filter(|s| s.input == input).map(|s| &s.endpoint).collect::<std::collections::BTreeSet<_>>().len() > 1
    });
    (Route { microphone, playback, application: members.first().map(|s| s.app.clone()),
        warning: ambiguous.then(|| crate::i18n::t("The call app is using multiple audio devices. Choose the sources in Settings.")) }, Some(family))
}

#[cfg(not(windows))]
fn current() -> Route { Route::default() }

#[cfg(windows)]
fn current() -> Route {
    use std::{sync::{Mutex, OnceLock}, time::{Duration, Instant}};
    struct Cached { at: Instant, route: Route, family: Option<u32> }
    static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if let Some(cached) = cache.as_ref().filter(|c| c.at.elapsed() < Duration::from_millis(750)) { return cached.route.clone(); }
    let previous = cache.as_ref().filter(|c| c.at.elapsed() < Duration::from_secs(5)).and_then(|c| c.family);
    let route = sessions().map(|sessions| choose(&sessions, previous)).unwrap_or_else(|_| (
        Route { warning: Some(crate::i18n::t("Call device detection is unavailable. Check the selected sources.")), ..Default::default() }, None));
    *cache = Some(Cached { at: Instant::now(), route: route.0.clone(), family: route.1 });
    route.0
}

#[cfg(windows)]
fn sessions() -> windows::core::Result<Vec<Session>> {
    use windows::{core::Interface, Win32::{Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator,
        IAudioSessionControl2, IAudioSessionManager2, AudioSessionStateActive, DEVICE_STATE_ACTIVE, eRender, eCapture},
        System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED}}};
    struct Com(bool);
    impl Drop for Com { fn drop(&mut self) { if self.0 { unsafe { CoUninitialize(); } } } }
    unsafe {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let mut raw = Vec::new();
        for (input, flow) in [(true, eCapture), (false, eRender)] {
            let devices = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
            for index in 0..devices.GetCount()? {
                let device = devices.Item(index)?;
                let raw_id = device.GetId()?;
                let id = raw_id.to_string();
                CoTaskMemFree(Some(raw_id.0.cast()));
                let id = format!("wasapi:{}", id?);
                let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let sessions = manager.GetSessionEnumerator()?;
                for index in 0..sessions.GetCount()? {
                    let control = sessions.GetSession(index)?.cast::<IAudioSessionControl2>()?;
                    if control.GetState()? != AudioSessionStateActive { continue; }
                    let pid = control.GetProcessId()?;
                    if pid != 0 && pid != std::process::id() { raw.push((pid, input, id.clone())); }
                }
            }
        }
        let families = crate::platform::audio_process_families(&raw.iter().map(|(pid, _, _)| *pid).collect::<Vec<_>>());
        Ok(raw.into_iter().filter_map(|(pid, input, endpoint)| {
            let (family, app) = families.get(&pid)?;
            Some(Session { family: *family, app: app.clone(), input, endpoint })
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session(family: u32, input: bool, endpoint: &str) -> Session {
        Session { family, app: format!("app-{family}"), input, endpoint: endpoint.into() }
    }

    #[test]
    fn a_call_follows_app_device_changes_even_when_system_defaults_do_not_change() {
        let (first, family) = choose(&[session(1, true, "laptop"), session(1, false, "buds"), session(2, false, "speakers")], None);
        assert_eq!(first.microphone.as_deref(), Some("laptop"));
        assert_eq!(first.playback.as_deref(), Some("buds"));
        let (changed, _) = choose(&[session(1, true, "usb"), session(1, false, "wired")], family);
        assert_eq!(changed.microphone.as_deref(), Some("usb"));
        assert_eq!(changed.playback.as_deref(), Some("wired"));
    }

    #[test]
    fn a_muted_call_retains_its_playback_family_and_does_not_follow_music() {
        let (route, _) = choose(&[session(1, false, "buds"), session(2, false, "speakers")], Some(1));
        assert_eq!(route.playback.as_deref(), Some("buds"));
        assert_eq!(route.microphone, None);
        assert!(choose(&[session(2, false, "speakers")], Some(1)).0.playback.is_none());
    }

    #[test]
    fn competing_calls_and_browser_outputs_are_reported_instead_of_guessed() {
        let (calls, _) = choose(&[session(1, true, "laptop"), session(2, true, "usb")], None);
        assert!(calls.warning.is_some());
        assert!(calls.microphone.is_none());
        let (tabs, _) = choose(&[session(1, true, "laptop"), session(1, false, "buds"), session(1, false, "speakers")], None);
        assert!(tabs.warning.is_some());
        assert_eq!(tabs.microphone.as_deref(), Some("laptop"));
        assert!(tabs.playback.is_none());
    }

    #[test]
    fn fixed_sources_do_not_consult_or_follow_another_apps_routing() {
        for source in [Source::Mic(Some("laptop".into())), Source::System(Some("buds".into()))] {
            let (resolved, application, warning) = resolve(&source);
            assert_eq!(resolved, source);
            assert!(application.is_none() && warning.is_none());
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Inspects real Windows audio sessions; explicit hardware diagnostic"]
    fn live_call_audio_routes() {
        println!("sessions: {:#?}", sessions().unwrap());
        println!("selected: {:#?}", current());
    }
}
