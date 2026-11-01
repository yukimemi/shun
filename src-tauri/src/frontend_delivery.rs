//! Startup UI notifications are retained until the frontend listener exists.
//! Readiness never changes native visibility, so a hidden window stays hidden.
#[derive(Default)]
pub(crate) struct FrontendDelivery {
    ready: bool,
    pending: bool,
}

impl FrontendDelivery {
    pub(crate) fn shown(&mut self) -> bool {
        self.pending = !self.ready;
        self.ready
    }

    pub(crate) fn ready(&mut self, visible: bool) -> bool {
        self.ready = true;
        std::mem::take(&mut self.pending) && visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_show_delivers_once_after_listener_registration() {
        let mut delivery = FrontendDelivery::default();
        assert!(!delivery.shown());
        assert!(delivery.ready(true));
        assert!(!delivery.ready(true));
        assert!(delivery.shown());
    }

    #[test]
    fn cold_toggle_then_hide_discards_deferred_ui_work() {
        let mut delivery = FrontendDelivery::default();
        assert!(!delivery.shown());
        assert!(!delivery.ready(false));
        assert!(!delivery.ready(true));
        assert!(delivery.shown());
    }

    #[test]
    fn cold_hotkey_and_resident_have_no_show_to_deliver() {
        let mut delivery = FrontendDelivery::default();
        assert!(!delivery.ready(false));
        assert!(!delivery.ready(true));
    }

    #[test]
    fn show_hide_show_before_ready_delivers_current_show() {
        let mut delivery = FrontendDelivery::default();
        assert!(!delivery.shown());
        assert!(!delivery.shown());
        assert!(delivery.ready(true));
        assert!(!delivery.ready(true));
    }
}
