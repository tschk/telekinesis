//! Native menu commands share the same handlers as the Crepuscularity controls.
use super::*;

actions!(
    native_workspace,
    [
        OpenProject,
        NewThread,
        RenameThread,
        HandoffThread,
        AttachReference,
        RemoveReference,
        CloseWorkspace,
        ToggleSidebar,
        ToggleContext,
        Settings,
        SendMessage,
        QueueMode,
        CycleRole,
        CyclePermission,
        StopWork,
        Minimize,
        Zoom,
        Fullscreen,
        ShowWorkspace,
        CompactWindow,
        StandardWindow,
        CancelEdit,
        FindThread
    ]
);

pub(super) fn install(handle: WindowHandle<Workspace>, cx: &mut App) {
    cx.on_action(move |_: &CloseWorkspace, cx| {
        cx.defer(move |cx| {
            let _ = handle.update(cx, |this, window, cx| this.request_close(window, cx));
        });
    });
    cx.on_action(move |_: &ShowWorkspace, cx| {
        cx.activate(true);
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        });
    });
    // Native menu activation can leave no focused element after a macOS file panel.
    // Defer the exact window update, as for Close, to avoid a nested WindowHandle borrow.
    cx.on_action(move |_: &AttachReference, cx| {
        cx.defer(move |cx| {
            let _ = handle.update(cx, |this, window, cx| {
                this.attach_context(&ClickEvent::default(), window, cx)
            });
        });
    });
    cx.on_action(move |_: &RemoveReference, cx| {
        cx.defer(move |cx| {
            let _ = handle.update(cx, |this, window, cx| {
                this.remove_context(&ClickEvent::default(), window, cx)
            });
        });
    });
    cx.bind_keys([
        KeyBinding::new("cmd-f", FindThread, None),
        KeyBinding::new("cmd-o", OpenProject, None),
        KeyBinding::new("cmd-n", NewThread, None),
        KeyBinding::new("cmd-w", CloseWorkspace, None),
        KeyBinding::new("cmd-q", CloseWorkspace, None),
        KeyBinding::new("cmd-comma", Settings, None),
        KeyBinding::new("cmd-i", ToggleContext, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("ctrl-cmd-f", Fullscreen, None),
        KeyBinding::new("cmd-0", ShowWorkspace, None),
        KeyBinding::new("ctrl-c", StopWork, None),
        KeyBinding::new("escape", CancelEdit, Some("Workspace")),
        KeyBinding::new("alt-cmd-1", CompactWindow, None),
        KeyBinding::new("alt-cmd-2", StandardWindow, None),
    ]);
    cx.set_menus(vec![
        Menu {
            name: "Telekinesis".into(),
            items: vec![
                MenuItem::action("Settings…", Settings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Telekinesis", CloseWorkspace),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Open Project…", OpenProject),
                MenuItem::action("New Thread", NewThread),
                MenuItem::action("Rename Thread…", RenameThread),
                MenuItem::action("Handoff…", HandoffThread),
                MenuItem::action("Attach Selected Reference", AttachReference),
                MenuItem::action("Remove Reference", RemoveReference),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWorkspace),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Cut", editor::Cut),
                MenuItem::action("Copy", editor::Copy),
                MenuItem::action("Paste", editor::Paste),
                MenuItem::action("Select All", editor::SelectAll),
                MenuItem::separator(),
                MenuItem::action("Find Thread…", FindThread),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Show or Hide Sidebar", ToggleSidebar),
                MenuItem::action("Show or Hide Context", ToggleContext),
                MenuItem::action("Thread Settings", Settings),
            ],
        },
        Menu {
            name: "Thread".into(),
            items: vec![
                MenuItem::action("Send Message", SendMessage),
                MenuItem::action("Toggle Queue or Steer", QueueMode),
                MenuItem::action("Change Role", CycleRole),
                MenuItem::action("Change Permission", CyclePermission),
                MenuItem::action("Stop", StopWork),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::action("Toggle Full Screen", Fullscreen),
                MenuItem::separator(),
                MenuItem::action("Compact Window", CompactWindow),
                MenuItem::action("Standard Window", StandardWindow),
                MenuItem::separator(),
                MenuItem::action("Show Workspace", ShowWorkspace),
            ],
        },
    ]);
}

pub(super) fn bind(root: Stateful<Div>, cx: &mut Context<Workspace>) -> Stateful<Div> {
    root.on_action(cx.listener(|this, _: &FindThread, window, cx| {
        window.focus(&this.search.read(cx).focus_handle)
    }))
    .on_action(cx.listener(|this, _: &OpenProject, window, cx| {
        this.open_project(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &NewThread, window, cx| {
        this.new_thread(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &RenameThread, window, cx| {
        this.start_rename(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &HandoffThread, window, cx| {
        this.start_handoff(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &ToggleSidebar, window, cx| {
        this.toggle_sidebar(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &ToggleContext, window, cx| {
        this.toggle_context(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &Settings, window, cx| {
        this.toggle_settings(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &SendMessage, window, cx| {
        this.send_click(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &QueueMode, window, cx| {
        this.toggle_queue(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &CycleRole, window, cx| {
        this.cycle_role(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &CyclePermission, window, cx| {
        this.cycle_permission(&ClickEvent::default(), window, cx)
    }))
    .on_action(
        cx.listener(|this, _: &StopWork, window, cx| this.stop(&ClickEvent::default(), window, cx)),
    )
    .on_action(cx.listener(|this, _: &CancelEdit, window, cx| {
        this.cancel_handoff(&ClickEvent::default(), window, cx)
    }))
    .on_action(cx.listener(|this, _: &CloseWorkspace, window, cx| this.request_close(window, cx)))
    .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
    .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
    .on_action(cx.listener(|_, _: &Fullscreen, window, _| window.toggle_fullscreen()))
    .on_action(cx.listener(|_, _: &ShowWorkspace, window, cx| {
        cx.activate(true);
        window.activate_window();
    }))
    .on_action(
        cx.listener(|_, _: &CompactWindow, window, _| {
            resize_frame(window, size(px(900.), px(650.)))
        }),
    )
    .on_action(cx.listener(|_, _: &StandardWindow, window, _| {
        resize_frame(window, size(px(1512.), px(949.)))
    }))
}

// Public resize takes content size; keep menu targets stable when native chrome changes.
pub(super) fn resize_frame(window: &mut Window, desired: Size<Pixels>) {
    let chrome = window.bounds().size - window.viewport_size();
    window.resize(size(
        (desired.width - chrome.width).max(px(1.)),
        (desired.height - chrome.height).max(px(1.)),
    ));
}
