use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
use security_framework::os::macos::keychain::SecKeychain;
use std::io;

pub(super) fn password(service: &str, interactive: bool) -> io::Result<Option<Vec<u8>>> {
    // Claude uses the legacy login keychain. Suppress its dialogs as well as modern
    // authenticated-item UI. This guard is scoped to this short-lived command process.
    let _no_ui = (!interactive)
        .then(SecKeychain::disable_user_interaction)
        .transpose()
        .map_err(|e| io::Error::other(format!("Keychain access failed ({})", e.code())))?;
    let result = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(service)
        .load_data(true)
        .skip_authenticated_items(!interactive)
        .search();
    match result {
        Ok(items) => Ok(items.into_iter().find_map(|item| match item {
            SearchResult::Data(data) => Some(data),
            _ => None,
        })),
        Err(error) if error.code() == -25300 => Ok(None),
        Err(error) => Err(io::Error::other(format!(
            "Keychain access failed ({})",
            error.code()
        ))),
    }
}
