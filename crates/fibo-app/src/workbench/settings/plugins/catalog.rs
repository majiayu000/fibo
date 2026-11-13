//! Everything the plugin page lists, one `Item` per plugin whether it is installed, on offer
//! or built in, so 发现, 已安装 and the detail view say the same thing about it.
use super::*;
use fibo_core::effect::{self, Pick, Style};
use fibo_core::plugin::Plugin;
use fibo_core::plugin_category::Category;

#[derive(Clone)]
pub(super) struct Item {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) version: Option<String>,
    pub(super) provides: Vec<Kind>,
    pub(super) touches: Vec<String>,
    pub(super) network: Option<bool>,
    pub(super) category: Category,
    /// Installed and loaded, on or off.
    pub(super) plugin: Option<Plugin>,
    /// What can be installed under this id: not installed yet, or a newer version.
    pub(super) offer: Option<Offer>,
    /// `offer` is newer than what is installed.
    pub(super) update: bool,
    /// How it plays, for an animation that can be shown.
    pub(super) complete: Option<Complete>,
    /// How to make it the one playing, when it can be right now.
    pub(super) pick: Option<Pick>,
}
impl Item {
    pub(super) fn builtin(&self) -> bool {
        self.plugin.is_none() && self.offer.is_none()
    }
    pub(super) fn installed(&self) -> bool {
        self.plugin.is_some() || self.builtin()
    }
    pub(super) fn enabled(&self) -> bool {
        self.plugin.as_ref().is_none_or(|p| p.enabled)
    }
    /// What it adds and whether it goes online, in the words the install question uses.
    pub(super) fn safety(&self) -> String {
        let change = if self.touches.is_empty() {
            t!("不改别的程序的文件").to_owned()
        } else {
            t!("会改 {files}", files = self.touches.join(t!("、")))
        };
        let online = match self.network {
            Some(true) => t!("要联网"),
            Some(false) => t!("声明不联网"),
            None if market::runs_commands(&self.provides) => t!("没说是否联网"),
            None => t!("不联网"),
        };
        t!("{change}，{online}", change, online)
    }
    pub(super) fn answers(&self, query: &str) -> bool {
        query.is_empty()
            || format!(
                "{} {}",
                self.name,
                self.description.as_deref().unwrap_or_default()
            )
            .to_lowercase()
            .contains(query)
    }
}

/// What is playing: a picked plugin since turned off or removed plays the seal.
pub(super) fn playing() -> Pick {
    let presets = effect::presets();
    match effect::picked() {
        Some(Pick::Plugin(id)) if !presets.iter().any(|p| p.id == id) => Pick::Builtin(Style::Seal),
        Some(p) => p,
        None => Pick::Builtin(Style::Seal),
    }
}

/// Every plugin once, in the order every list shows them.
pub(super) fn items(offers: &[Offer]) -> Vec<Item> {
    let presets = effect::presets();
    let style_of = |id: &str| id.strip_prefix("complete-").and_then(Style::parse);
    let mut out: Vec<Item> = Vec::new();
    for p in plugin::report()
        .iter()
        .filter_map(|f| f.result.as_ref().ok())
    {
        let preset = presets.iter().find(|x| x.id == p.id);
        let update = offers.iter().find(|o| {
            o.entry.id == p.id
                && p.version
                    .as_deref()
                    .is_none_or(|v| market::newer(&o.entry.version, v))
        });
        out.push(Item {
            id: p.id.clone(),
            name: short_name(&p.name),
            description: p.description.clone(),
            version: p.version.clone(),
            provides: p.provides.clone(),
            touches: p.touches.clone(),
            network: p.network,
            category: Category::of(&p.provides),
            plugin: Some(p.clone()),
            offer: update.cloned(),
            update: update.is_some(),
            complete: preset
                .map(|x| x.complete)
                .or_else(|| style_of(&p.id).map(Complete::of)),
            pick: preset.map(|x| Pick::Plugin(x.id.clone())),
        });
    }
    for o in offers {
        if out.iter().any(|i| i.id == o.entry.id) {
            continue;
        }
        let e = &o.entry;
        out.push(Item {
            id: e.id.clone(),
            name: short_name(&e.name),
            description: e.description.clone(),
            version: Some(e.version.clone()),
            provides: e.provides.clone(),
            touches: e.touches.clone(),
            network: e.network,
            category: Category::of(&e.provides),
            plugin: None,
            offer: Some(o.clone()),
            update: false,
            complete: style_of(&e.id).map(Complete::of),
            pick: None,
        });
    }
    for s in Style::BUILTIN {
        out.push(Item {
            id: s.name().to_owned(),
            name: s.label().to_owned(),
            description: Some(
                match s {
                    Style::Fade => t!("内置；系统开了「减弱动态效果」时都播它"),
                    _ => t!("内置；按钮填满，印章盖在窗格上，窗格淡出"),
                }
                .to_owned(),
            ),
            version: None,
            provides: vec![Kind::Effects],
            touches: vec![],
            network: Some(false),
            category: Category::Effects,
            plugin: None,
            offer: None,
            update: false,
            complete: Some(Complete::of(s)),
            pick: Some(Pick::Builtin(s)),
        });
    }
    // The faces first: they are what the page is most often opened to look at.
    out.sort_by_key(|i| i.complete.is_none_or(|x| !Style::FACES.contains(&x.style)));
    out
}
