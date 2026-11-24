//! Where the user is: the space, tab and pane in view, or the overview.

mod back;

/// Where the user is. Every view reads it; the shell moves it.
pub(super) struct Nav {
    pub(super) space: String,
    pub(super) tab: Option<String>,
    pub(super) focused: Option<String>,
    pub(super) pending_focus: bool,
    pub(super) zoomed: Option<String>,
    pub(super) overview: bool,
    pub(super) task_detail: Option<String>,
    /// Where ⌘J or a notification jumped from, for 回到刚才.
    pub(super) trail: Trail,
}
/// Space, tab, overview, task detail: the page the user chose.
pub(super) type Page = (String, Option<String>, bool, Option<String>);
impl Nav {
    /// The page the user chose: a change means they went somewhere themselves.
    pub(super) fn page(&self) -> Page {
        (
            self.space.clone(),
            self.tab.clone(),
            self.overview,
            self.task_detail.clone(),
        )
    }
}

/// A place to come back to: another machine's main area or this one's page, the pane
/// that had input and the board's 你在这里 card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Spot {
    pub(super) host: Option<String>,
    pub(super) page: Page,
    pub(super) focused: Option<String>,
    pub(super) here: Option<String>,
}

/// 回到刚才 (#515): ⌘J and a notification swap the view for the session that waits, so the
/// spot they left from is kept here. LastTab is the row's last tab, not this.
#[derive(Default)]
pub(super) struct Trail {
    back: Option<Spot>,
    /// What the last jump went to. Pressing ⌘J again while still on it goes on with the
    /// same trip, so the way back stays where the trip began.
    landed: Option<String>,
}
impl Trail {
    /// A jump from `from` to `to`; `here` is what `from` shows (a session or terminal),
    /// as ⌘J tells which one it is on.
    pub(super) fn jump(&mut self, from: Spot, here: Option<&str>, to: &str) {
        // A notification for the session already in view moves nothing.
        if here == Some(to) {
            return;
        }
        let going_on =
            self.back.is_some() && self.landed.is_some() && self.landed.as_deref() == here;
        if !going_on {
            self.back = Some(from);
        }
        self.landed = Some(to.to_owned());
    }
    /// The spot to go back to, if a jump left one; the spot left now becomes the way back,
    /// so pressing again returns, as cmux's ⌘⇧U does.
    pub(super) fn back(&mut self, from: Spot) -> Option<Spot> {
        let to = self.back.take()?;
        self.back = Some(from);
        self.landed = None;
        Some(to)
    }
}

#[cfg(test)]
mod tests;
