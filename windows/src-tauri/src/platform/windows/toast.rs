// Windows toast notifications for an app that is not packaged.
//
// A toast needs an app identity (AUMID). A packaged app has one; this one
// registers its bundle identifier under HKCU\Software\Classes\AppUserModelId
// with a display name and an icon, which is all an unpackaged app needs, and
// sets it on the process. Measured on Windows 11: the toast is delivered, and a
// click on its button is reported to this process with the button's arguments.
//
// Every WinRT call runs on one thread of its own, in the multithreaded
// apartment. The toast objects stay there too: a toast that is dropped stops
// reporting clicks.

use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

use ::windows::core::{Interface, HSTRING, IInspectable, Ref};
use ::windows::Data::Xml::Dom::XmlDocument;
use ::windows::Foundation::TypedEventHandler;
use ::windows::Win32::Foundation::ERROR_SUCCESS;
use ::windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_SZ,
};
use ::windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use ::windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use ::windows::UI::Notifications::{ToastActivatedEventArgs, ToastNotification, ToastNotificationManager};

use crate::shell::{self, Activation, ToastSpec};

/// Every toast of this app is in one group, so one can be taken back by its tag.
const GROUP: &str = "coucou";
/// Toasts kept alive for their clicks; older ones have long left the screen.
const KEEP: usize = 16;

enum Msg {
    Show(ToastSpec),
    Clear(String),
}

static SENDER: OnceLock<Mutex<Sender<Msg>>> = OnceLock::new();

pub type OnActivate = Box<dyn Fn(Activation) + Send + Sync + 'static>;

/// Registers the app identity and starts the toast thread. Called once.
pub fn start(aumid: String, display_name: String, icon: std::path::PathBuf, on_activate: OnActivate) {
    let (tx, rx) = channel::<Msg>();
    if SENDER.set(Mutex::new(tx)).is_err() {
        return;
    }
    let on_activate = std::sync::Arc::new(on_activate);
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
            let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(&aumid));
        }
        if let Err(err) = register(&aumid, &display_name, &icon) {
            crate::log::line(format!("toast: could not register the app identity: {err}"));
        }
        let mut alive: Vec<ToastNotification> = Vec::new();
        for msg in rx {
            match msg {
                Msg::Show(spec) => match show_now(&aumid, &spec, on_activate.clone()) {
                    Ok(toast) => {
                        alive.push(toast);
                        if alive.len() > KEEP {
                            alive.remove(0);
                        }
                    }
                    Err(err) => crate::log::line(format!("toast: not shown: {err}")),
                },
                Msg::Clear(tag) => {
                    if let Ok(history) = ToastNotificationManager::History() {
                        let _ = history.RemoveGroupedTagWithId(&HSTRING::from(&tag), &HSTRING::from(GROUP), &HSTRING::from(&aumid));
                    }
                }
            }
        }
    });
}

fn send(msg: Msg) -> Result<(), String> {
    let sender = SENDER.get().ok_or("notifications are not ready")?;
    sender.lock().unwrap().send(msg).map_err(|_| "the notification thread is gone".to_string())
}

pub fn show(spec: ToastSpec) -> Result<(), String> {
    send(Msg::Show(spec))
}

pub fn clear(tag: String) {
    let _ = send(Msg::Clear(tag));
}

fn show_now(aumid: &str, spec: &ToastSpec, on_activate: std::sync::Arc<OnActivate>) -> ::windows::core::Result<ToastNotification> {
    let xml = XmlDocument::new()?;
    xml.LoadXml(&HSTRING::from(shell::toast_xml(spec)))?;
    let toast = ToastNotification::CreateToastNotification(&xml)?;
    toast.SetTag(&HSTRING::from(spec.tag.as_str()))?;
    toast.SetGroup(&HSTRING::from(GROUP))?;
    toast.Activated(&TypedEventHandler::new(move |_: Ref<ToastNotification>, args: Ref<IInspectable>| {
        let arguments = args
            .as_ref()
            .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
            .and_then(|a| a.Arguments().ok())
            .map(|h| h.to_string())
            .unwrap_or_default();
        on_activate(shell::parse_activation(&arguments));
        Ok(())
    }))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid))?.Show(&toast)?;
    Ok(toast)
}

/// HKCU\Software\Classes\AppUserModelId\<aumid>: DisplayName and IconUri.
fn register(aumid: &str, display_name: &str, icon: &Path) -> Result<(), String> {
    let key_path = format!(r"Software\Classes\AppUserModelId\{aumid}");
    unsafe {
        let mut key = HKEY::default();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key_path.as_str()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        );
        if status != ERROR_SUCCESS {
            return Err(format!("RegCreateKeyExW: {}", status.0));
        }
        let mut result = Ok(());
        for (name, value) in [("DisplayName", display_name.to_string()), ("IconUri", icon.to_string_lossy().to_string())] {
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let status = RegSetValueExW(key, &HSTRING::from(name), None, REG_SZ, Some(bytes));
            if status != ERROR_SUCCESS {
                result = Err(format!("RegSetValueExW {name}: {}", status.0));
            }
        }
        let _ = RegCloseKey(key);
        result
    }
}
