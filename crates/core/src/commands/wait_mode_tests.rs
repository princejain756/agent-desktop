use super::*;
use crate::commands::wait::{WaitModeArgs, WaitPredicateArgs};

fn args(mode: WaitModeArgs) -> WaitArgs {
    WaitArgs {
        mode,
        predicate: WaitPredicateArgs {
            predicate: None,
            value: None,
            action: None,
            count: None,
        },
        timeout_ms: 1_000,
        app: None,
    }
}

fn mode() -> WaitModeArgs {
    WaitModeArgs {
        ms: None,
        element: None,
        window: None,
        text: None,
        surface: None,
        event: None,
        window_id: None,
    }
}

#[test]
fn event_with_window_title_narrowing_is_a_single_mode() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        event: Some("window-opened".into()),
        window: Some("Untitled".into()),
        ..mode()
    }));
    assert!(
        result.is_ok(),
        "--window alongside --event must narrow the event wait, not count as a second mode: {result:?}"
    );
}

#[test]
fn event_alone_is_still_a_single_mode() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        event: Some("window-opened".into()),
        ..mode()
    }));
    assert!(result.is_ok());
}

#[test]
fn window_and_element_together_remain_ambiguous() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        window: Some("Untitled".into()),
        element: Some("@e1".into()),
        ..mode()
    }));
    assert!(result.is_err());
}

#[test]
fn from_args_threads_window_title_into_event_mode() {
    let parsed = WaitMode::from_args(args(WaitModeArgs {
        event: Some("window-opened".into()),
        window: Some("Untitled".into()),
        ..mode()
    }))
    .unwrap();
    match parsed {
        WaitMode::Event {
            event,
            window_title,
            ..
        } => {
            assert_eq!(event, "window-opened");
            assert_eq!(window_title.as_deref(), Some("Untitled"));
        }
        _ => panic!("expected WaitMode::Event, got a different mode"),
    }
}

#[test]
fn from_args_maps_surface_variants_to_menu_open_state() {
    let open = WaitMode::from_args(args(WaitModeArgs {
        surface: Some(SurfaceWait::Menu),
        ..mode()
    }))
    .unwrap();
    assert!(matches!(open, WaitMode::Menu { open: true, .. }));

    let closed = WaitMode::from_args(args(WaitModeArgs {
        surface: Some(SurfaceWait::MenuClosed),
        ..mode()
    }))
    .unwrap();
    assert!(matches!(closed, WaitMode::Menu { open: false, .. }));
}

#[test]
fn from_args_threads_text_filter_into_notification_mode() {
    let parsed = WaitMode::from_args(args(WaitModeArgs {
        surface: Some(SurfaceWait::Notification),
        text: Some("done".into()),
        ..mode()
    }))
    .unwrap();
    match parsed {
        WaitMode::Notification { text, .. } => assert_eq!(text.as_deref(), Some("done")),
        _ => panic!("expected WaitMode::Notification, got a different mode"),
    }
}

#[test]
fn surface_and_element_together_remain_ambiguous() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        surface: Some(SurfaceWait::Menu),
        element: Some("@e1".into()),
        ..mode()
    }));

    assert_eq!(result.unwrap_err().code(), "INVALID_ARGS");
}

#[test]
fn app_event_rejects_window_filter_immediately() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        event: Some("app-launched".into()),
        window_id: Some("w-1".into()),
        ..mode()
    }));

    assert_eq!(result.unwrap_err().code(), "INVALID_ARGS");
}

#[test]
fn surface_event_requires_app_scope() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        event: Some("surface-appeared".into()),
        ..mode()
    }));

    assert_eq!(result.unwrap_err().code(), "INVALID_ARGS");
}

#[test]
fn surface_event_accepts_app_scope() {
    let mut request = args(WaitModeArgs {
        event: Some("surface-dismissed".into()),
        ..mode()
    });
    request.app = Some("TextEdit".into());

    assert!(validate_wait_mode(&request).is_ok());
}

#[test]
fn orphan_action_flag_is_rejected() {
    let mut request = args(WaitModeArgs {
        ms: Some(0),
        ..mode()
    });
    request.predicate.action = Some("type".into());

    let err = validate_wait_mode(&request).unwrap_err();

    assert_eq!(err.code(), "INVALID_ARGS");
    assert!(err.to_string().contains("--action requires --element"));
}

#[test]
fn orphan_window_id_is_rejected() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        ms: Some(0),
        window_id: Some("w-1".into()),
        ..mode()
    }));

    let err = result.unwrap_err();

    assert_eq!(err.code(), "INVALID_ARGS");
    assert!(err.to_string().contains("--window-id requires --event"));
}

#[test]
fn action_on_element_wait_is_still_accepted() {
    let mut request = args(WaitModeArgs {
        element: Some("@e1".into()),
        ..mode()
    });
    request.predicate.predicate = Some("actionable".into());
    request.predicate.action = Some("type".into());

    assert!(validate_wait_mode(&request).is_ok());
}

#[test]
fn window_id_scoped_event_wait_is_still_accepted() {
    let result = validate_wait_mode(&args(WaitModeArgs {
        event: Some("window-closed".into()),
        window_id: Some("w-1".into()),
        ..mode()
    }));

    assert!(result.is_ok());
}
