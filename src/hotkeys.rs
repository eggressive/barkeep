use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;

/// Registered global hotkeys and their ids.
pub struct Hotkeys {
    _manager: GlobalHotKeyManager,
    pub toggle_id: u32,
}

/// Register Ctrl+Space (summon the palette).
pub fn register() -> Result<Hotkeys, String> {
    let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
    let hotkey = HotKey::new(Some(Modifiers::CONTROL), Code::Space);
    manager.register(hotkey).map_err(|e| e.to_string())?;
    Ok(Hotkeys {
        _manager: manager,
        toggle_id: hotkey.id(),
    })
}
