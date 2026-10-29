use std::{
    mem,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Default)]
struct NavigationState {
    reserved: bool,
    requested: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PlaybackControl {
    skip_current: Arc<AtomicBool>,
    restart: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    navigation: Arc<Mutex<NavigationState>>,
}

impl PlaybackControl {
    #[cfg(any(feature = "tokio", test))]
    pub(crate) fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Interrupt the active file or scheduled live source.
    pub fn skip_current(&self) {
        self.skip_current.store(true, Ordering::SeqCst);
    }

    /// Withdraw an unconsumed skip when its intended source already ended.
    pub fn cancel_skip(&self) {
        self.skip_current.store(false, Ordering::SeqCst);
    }

    pub fn restart_playout(&self) {
        self.restart.store(true, Ordering::SeqCst);
    }

    /// Reserve navigation until playlist and database changes are committed.
    pub fn begin_navigation(&self) -> Result<PlaylistNavigation, NavigationBlocked> {
        if self.is_shutdown() {
            return Err(NavigationBlocked::Busy);
        }
        let mut state = self
            .navigation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.reserved {
            return Err(NavigationBlocked::Busy);
        }
        state.reserved = true;

        Ok(PlaylistNavigation {
            control: self.clone(),
        })
    }

    pub(crate) fn take_skip_current(&self) -> bool {
        self.skip_current.swap(false, Ordering::SeqCst)
    }

    pub(crate) fn take_restart(&self) -> bool {
        self.restart.swap(false, Ordering::SeqCst)
    }

    pub(crate) fn take_navigation(&self) -> bool {
        let mut state = self
            .navigation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if state.reserved {
            return false;
        }

        mem::take(&mut state.requested)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum NavigationBlocked {
    Busy,
}

pub struct PlaylistNavigation {
    control: PlaybackControl,
}

impl PlaylistNavigation {
    pub fn commit(self) {
        let mut state = self
            .control
            .navigation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.requested = true;
        // The lease releases the reservation after this lock, once updates finish.
    }
}

impl Drop for PlaylistNavigation {
    fn drop(&mut self) {
        self.control
            .navigation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reserved = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_becomes_visible_only_after_commit() {
        let control = PlaybackControl::default();
        let navigation = control.begin_navigation().unwrap();
        assert!(!control.take_navigation());
        assert!(matches!(
            control.begin_navigation(),
            Err(NavigationBlocked::Busy)
        ));
        navigation.commit();
        assert!(control.take_navigation());
        assert!(!control.take_navigation());
        assert!(!control.take_skip_current());
    }

    #[test]
    fn cancelled_navigation_releases_without_skipping() {
        let control = PlaybackControl::default();
        drop(control.begin_navigation().unwrap());
        assert!(!control.take_navigation());
        assert!(control.begin_navigation().is_ok());
    }

    #[test]
    fn shutdown_remains_effective_across_playback_checks() {
        let control = PlaybackControl::default();
        control.request_shutdown();
        assert!(matches!(
            control.begin_navigation(),
            Err(NavigationBlocked::Busy)
        ));
        for _ in 0..3 {
            assert!(
                crate::playout::check_playback_control(&control)
                    .unwrap_err()
                    .is::<crate::playout::PlaybackRestart>()
            );
        }
    }

    #[test]
    fn concurrent_navigation_has_one_owner() {
        let control = PlaybackControl::default();
        let worker_control = control.clone();
        let navigation = control.begin_navigation().unwrap();
        let worker = std::thread::spawn(move || {
            assert!(matches!(
                worker_control.begin_navigation(),
                Err(NavigationBlocked::Busy)
            ));
        });
        worker.join().unwrap();
        navigation.commit();
        assert!(control.take_navigation());
    }
}
