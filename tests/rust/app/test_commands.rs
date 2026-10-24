use super::*;

#[test]
fn specific_shortcuts_come_first() {
    let all = shortcuts();
    let position = |action: &Action| all.iter().position(|(a, _)| a == action).unwrap();
    assert!(position(&Action::View(StandardView::Front)) < position(&Action::Render));
    assert!(position(&Action::SaveAs) < position(&Action::Save));
    assert!(position(&Action::Stop) < position(&Action::ToggleDebug));
}

#[test]
fn menus_find_their_shortcuts() {
    assert_eq!(shortcut(&Action::Render), Some(key(Key::F5)));
    assert_eq!(shortcut(&Action::Autocomplete), Some(ctrl(Key::Space)));
    assert_eq!(shortcut(&Action::FindPrevious), Some(alt(Key::F3)));
    assert_eq!(shortcut(&Action::FindNext), Some(key(Key::F3)));
    assert_eq!(shortcut(&Action::Upload), None);
}

#[test]
fn selector_commands_have_menu_entries_and_a_free_shortcut() {
    assert_eq!(shortcut(&Action::InsertSelector), Some(ctrl_shift(Key::I)));
    // Copy has none: Ctrl+Shift+C would also copy the editor's text.
    assert_eq!(shortcut(&Action::CopySelector), None);
    let taken = shortcuts()
        .iter()
        .filter(|(_, binding)| *binding == ctrl_shift(Key::I))
        .count();
    assert_eq!(taken, 1, "Ctrl+Shift+I is bound once");
}

#[test]
fn legacy_shortcuts_are_preserved() {
    let all = shortcuts();
    let has = |action: Action, binding: KeyboardShortcut| {
        assert!(
            all.contains(&(action.clone(), binding)),
            "missing legacy shortcut for {action:?}"
        );
    };

    has(Action::NewFile, ctrl(Key::N));
    has(Action::OpenFile, ctrl(Key::O));
    has(Action::Save, ctrl(Key::S));
    has(Action::SaveAs, ctrl_shift(Key::S));
    has(Action::Quit, ctrl(Key::Q));
    has(Action::ToggleComment, ctrl(Key::Slash));
    has(Action::Autocomplete, alt(Key::Slash));
    has(Action::Find, ctrl(Key::F));

    has(Action::Render, key(Key::F5));
    has(Action::ToggleDebug, ctrl(Key::F5));
    has(Action::Debug(DebugAction::Step), ctrl(Key::F10));
    has(Action::Debug(DebugAction::StepIn), ctrl(Key::F11));
    has(Action::Debug(DebugAction::Continue), ctrl(Key::F12));

    has(Action::Fit, shift(Key::F1));
    for (view, key) in [
        (StandardView::Iso, Key::F2),
        (StandardView::Top, Key::F3),
        (StandardView::Bottom, Key::F4),
        (StandardView::Front, Key::F5),
        (StandardView::Back, Key::F6),
        (StandardView::Left, Key::F7),
        (StandardView::Right, Key::F8),
    ] {
        has(Action::View(view), shift(key));
    }
    has(Action::Display(DisplayMode::Wireframe), shift(Key::F9));
    has(Action::Display(DisplayMode::Shaded), shift(Key::F10));
}

#[test]
fn the_new_display_modes_follow_on_the_next_function_keys() {
    let has = |action: Action, expected| assert_eq!(shortcut(&action), Some(expected));
    has(Action::Display(DisplayMode::XRay), shift(Key::F11));
    has(Action::Display(DisplayMode::HiddenLine), shift(Key::F12));
    // Stepping out keeps Ctrl+Shift+F11: the more specific match wins.
    let order: Vec<_> = shortcuts().into_iter().map(|(action, _)| action).collect();
    let position = |action: &Action| order.iter().position(|a| a == action).unwrap();
    assert!(
        position(&Action::Debug(DebugAction::StepOut))
            < position(&Action::Display(DisplayMode::XRay))
    );
}

#[test]
fn go_to_source_has_its_own_shortcut() {
    assert_eq!(
        shortcut(&Action::GoToSelectionSource),
        Some(ctrl_shift(Key::G))
    );
    let taken = shortcuts()
        .iter()
        .filter(|(_, binding)| *binding == ctrl_shift(Key::G))
        .count();
    assert_eq!(taken, 1);
}

#[test]
fn profiling_has_its_own_shortcut_next_to_render() {
    assert_eq!(shortcut(&Action::Profile), Some(alt(Key::F5)));
    // Alt+F5 is matched before the plain F5 of Render.
    let order: Vec<_> = shortcuts().into_iter().map(|(action, _)| action).collect();
    let position = |action: &Action| order.iter().position(|a| a == action).unwrap();
    assert!(position(&Action::Profile) < position(&Action::Render));
}
