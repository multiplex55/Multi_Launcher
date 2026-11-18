//! Retained, local Inbox view. Effects are submitted to the owning service.
use crate::gui::{
    jump_mode::{
        TargetKind,
        ui::{
            JumpModeScope, NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
            register_and_apply_target,
        },
    },
    utility_host::{UtilityPresentation, presentation_button},
    utility_window_host::HostedUtilityDialog,
};
use crate::notifications::{
    Category, Content, FactId, InboxCommand, InboxServiceSnapshot, PersistenceStatus, Record,
    RelatedItem, Settings, Source, Status, WindowsAccess,
};
use eframe::egui;
use std::sync::Arc;

#[derive(Debug)]
pub(super) enum Command {
    Closed,
    RequestWindowsAccess,
    Submit(InboxCommand),
    OpenRelated(Record),
}
#[derive(Default)]
pub(super) struct Runtime {
    pub snapshot: Option<Arc<InboxServiceSnapshot>>,
    pub notice: Option<String>,
    /// Current Unix time for relative-date filtering.
    pub now_unix: Option<i64>,
    pub busy: bool,
    pub windows_access: WindowsAccess,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ReadFilter {
    #[default]
    All,
    Unread,
    Read,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DateFilter {
    #[default]
    All,
    Day,
    Week,
    Month,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Group {
    #[default]
    Date,
    Source,
    Category,
    App,
}
#[derive(Clone)]
enum ClearTarget {
    Source(Source),
    App(String),
    All,
}
#[derive(Clone, Default)]
struct View {
    selected: Option<FactId>,
    search: String,
    read: ReadFilter,
    date: DateFilter,
    source: Option<Source>,
    category: Option<Category>,
    app: String,
    group: Group,
    page: usize,
    settings: Option<Settings>,
    settings_base: Option<Settings>,
    clear: Option<ClearTarget>,
}
#[derive(Default)]
pub(super) struct NotificationInboxDialog {
    pub open: bool,
    session: u64,
    view: View,
}
impl NotificationInboxDialog {
    pub fn open(&mut self) {
        if !self.open {
            if let Some(next) = self.session.checked_add(1) {
                self.session = next;
                self.open = true;
                self.view = View::default();
            }
        }
    }
    pub fn ensure_open(&mut self) {
        self.open();
    }
    pub fn close(&mut self) {
        self.open = false;
        self.view = View::default();
    }
    pub fn session(&self) -> u64 {
        self.session
    }
}
#[derive(Default)]
pub(super) struct Actions {
    session: u64,
    view: View,
    snapshot: Option<Arc<InboxServiceSnapshot>>,
    close: bool,
    host_request: Option<UtilityPresentation>,
    intent: Option<Intent>,
}
#[derive(Clone)]
enum Intent {
    Read(bool),
    Pin(bool),
    Dismiss,
    OpenRelated,
    CreateTodo,
    Snooze(u8),
    ApplySettings,
    RequestWindowsAccess,
    BeginClear(ClearTarget),
    ConfirmClear,
    CancelClear,
}
impl HostedUtilityDialog for NotificationInboxDialog {
    const KIND: Option<NavigationDialogKind> = Some(NavigationDialogKind::NotificationInbox);
    const KEY: &'static str = "notification-inbox";
    const TITLE: &'static str = "Notification Inbox";
    const SIZE: egui::Vec2 = egui::vec2(760.0, 640.0);
    const MINIMUM: egui::Vec2 = egui::vec2(420.0, 320.0);
    const SCROLL_NATIVE_WINDOW: bool = false;
    const REGISTER_TARGETS_ON_REPLAY: bool = true;
    type Runtime = Runtime;
    type Actions = Actions;
    type Checkpoint = ();
    type Command = Command;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("notification-inbox-window"),
        )
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.session
    }
    fn parent_targets_allowed(&self, _: &Runtime) -> bool {
        true
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn accept_close(&mut self, _: bool) -> bool {
        if self.view.clear.take().is_some() {
            false
        } else {
            true
        }
    }
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        scopes: (Option<&JumpModeScope>, Option<&JumpModeScope>),
        _: NavigationOwner,
        parent: bool,
        _: NavigationOwner,
        _: bool,
        _: u64,
        runtime: &Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Actions {
        let mut view = self.view.clone();
        if let Some(snapshot) = &runtime.snapshot {
            if view
                .selected
                .as_ref()
                .is_some_and(|id| !snapshot.inbox.records().iter().any(|r| &r.id == id))
            {
                view.selected = None;
            }
            if view.settings.is_none() {
                view.settings = Some(snapshot.inbox.settings().clone());
                view.settings_base = view.settings.clone();
            } else if view.settings == view.settings_base
                && view.settings_base.as_ref() != Some(snapshot.inbox.settings())
            {
                view.settings = Some(snapshot.inbox.settings().clone());
                view.settings_base = view.settings.clone();
            }
        }
        let mut actions = Actions {
            session: self.session,
            view,
            snapshot: runtime.snapshot.clone(),
            ..Default::default()
        };
        let mut open = true;
        let mut show = |ui: &mut egui::Ui| {
            ui.add_enabled_ui(parent, |ui| {
                self.contents(ui, &mut actions, runtime, scopes.0, presentation)
            });
        };
        if let Some(ui) = child {
            show(ui);
        } else {
            egui::Window::new(Self::TITLE)
                .id(Self::layer().id)
                .open(&mut open)
                .default_size(Self::SIZE)
                .min_size(Self::MINIMUM)
                .show(ctx, show);
        }
        actions.close |= !open;
        actions
    }
    fn host_request(actions: &Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Command {
        Command::Closed
    }
    fn command_closes(command: &Command) -> bool {
        matches!(command, Command::Closed)
    }
    fn complete_actions(&mut self, actions: Actions, _: Option<usize>) -> Vec<Command> {
        if !self.open || self.session != actions.session {
            return Vec::new();
        }
        if actions.close {
            if self.view.clear.take().is_some() {
                return Vec::new();
            }
            self.close();
            return vec![Command::Closed];
        }
        self.view = actions.view;
        let Some(intent) = actions.intent else {
            return Vec::new();
        };
        match intent {
            Intent::BeginClear(target) => {
                self.view.clear = Some(target);
                return Vec::new();
            }
            Intent::CancelClear => {
                self.view.clear = None;
                return Vec::new();
            }
            Intent::ConfirmClear => {
                let Some(target) = self.view.clear.take() else {
                    return Vec::new();
                };
                return vec![Command::Submit(match target {
                    ClearTarget::Source(source) => InboxCommand::ClearSource(source),
                    ClearTarget::App(app) => InboxCommand::ClearWindowsApp(app),
                    ClearTarget::All => InboxCommand::ClearAll,
                })];
            }
            Intent::RequestWindowsAccess => {
                return vec![Command::RequestWindowsAccess];
            }
            Intent::ApplySettings => {
                let Some(snapshot) = actions.snapshot.as_ref() else {
                    return Vec::new();
                };
                if self.view.settings_base.as_ref() != Some(snapshot.inbox.settings()) {
                    self.view.settings = None;
                    self.view.settings_base = None;
                    return Vec::new();
                }
                let Some(settings) = self.view.settings.clone().filter(Settings::valid) else {
                    return Vec::new();
                };
                self.view.settings_base = None;
                self.view.settings = None;
                return vec![Command::Submit(InboxCommand::Settings(settings))];
            }
            _ => {}
        }
        let Some(record) = actions
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                self.view
                    .selected
                    .as_ref()
                    .and_then(|id| snapshot.inbox.records().iter().find(|r| &r.id == id))
            })
            .cloned()
        else {
            return Vec::new();
        };
        let command = match intent {
            Intent::Read(read) => Command::Submit(InboxCommand::SetRead {
                id: record.id,
                read,
            }),
            Intent::Pin(pinned) => Command::Submit(InboxCommand::SetPinned {
                id: record.id,
                pinned,
            }),
            Intent::Dismiss => Command::Submit(InboxCommand::Dismiss(record.id)),
            Intent::OpenRelated
                if record.related.is_some() && record.status != Status::SourceRemoved =>
            {
                Command::OpenRelated(record)
            }
            Intent::CreateTodo => Command::Submit(InboxCommand::CreateTodo { expected: record }),
            Intent::Snooze(minutes)
                if [5, 10, 15].contains(&minutes)
                    && record.status != Status::SourceRemoved
                    && matches!(
                        &record.related,
                        Some(RelatedItem::Timer { .. } | RelatedItem::Calendar { .. })
                    ) =>
            {
                Command::Submit(InboxCommand::Snooze {
                    expected: record,
                    minutes,
                })
            }
            _ => return Vec::new(),
        };
        vec![command]
    }
    #[cfg(test)]
    fn test_button(_: &Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        None
    }
}
fn date(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "Unknown time".into())
}
fn app(record: &Record) -> Option<&str> {
    match &record.id {
        FactId::Windows { app, .. } => Some(app),
        _ => None,
    }
}
fn summary(record: &Record) -> &str {
    match &record.content {
        Content::Internal(fact) => fact.summary(),
        Content::External { title, .. } => title.as_deref().unwrap_or("Windows notification"),
    }
}
fn group(record: &Record, grouping: Group) -> String {
    match grouping {
        Group::Date => date(record.occurred_at).chars().take(10).collect(),
        Group::Source => format!("{:?}", record.id.source()),
        Group::Category => format!("{:?}", record.category),
        Group::App => app(record).unwrap_or("Application sources").to_owned(),
    }
}
impl NotificationInboxDialog {
    fn contents(
        &self,
        ui: &mut egui::Ui,
        actions: &mut Actions,
        runtime: &Runtime,
        scope: Option<&JumpModeScope>,
        presentation: UtilityPresentation,
    ) {
        let mut order = 0;
        macro_rules! target {
            ($ui:expr, $r:expr, $id:expr, $label:expr, $kind:expr) => {
                register_and_apply_target(scope, $ui, &$r, $id, $label, $kind, &mut order);
            };
        }
        ui.horizontal_wrapped(|ui| {
            let (b, next) = presentation_button(ui, presentation, ui.is_enabled());
            target!(
                ui,
                b,
                "presentation",
                "Change window presentation",
                TargetKind::Button
            );
            if b.clicked() {
                actions.host_request = Some(next);
            }
            let b = ui.button("Close");
            target!(ui, b, "close", "Close", TargetKind::Button);
            actions.close |= b.clicked();
        });
        egui::ScrollArea::vertical().id_salt("notification-inbox-content").show(ui, |ui| {
            if let Some(notice)=&runtime.notice { ui.label(notice); }
            let Some(snapshot)=&runtime.snapshot else { ui.label("Inbox is unavailable."); return; };
            match snapshot.persistence { PersistenceStatus::Loading => {ui.label("Loading Inbox…");}, PersistenceStatus::MemoryOnly => {ui.label("Inbox history is unavailable; this session is memory only.");}, PersistenceStatus::WriteFailed => {ui.label("Inbox history could not be saved.");}, PersistenceStatus::Durable => {} }
            if snapshot.dropped_events > 0 { ui.label(format!("{} incoming notifications could not be retained.",snapshot.dropped_events)); }
            if snapshot.inbox.omitted() > 0 { ui.label(format!("Inbox capacity omitted {} notifications.", snapshot.inbox.omitted())); }
            if snapshot.calendar_warning { ui.label("Calendar reminders need attention."); }
            ui.label(format!("Windows notifications · {}", runtime.windows_access.label()));
            let records=snapshot.inbox.records(); let unread=snapshot.inbox.unmuted_unread_count();
            ui.heading(format!("Inbox — {unread} unread"));
            let search=ui.add(egui::TextEdit::singleline(&mut actions.view.search).hint_text("Search notifications").char_limit(256)); target!(ui,search,"search","Search notifications",TargetKind::TextEntry); if search.changed(){actions.view.page=0;}
            ui.horizontal_wrapped(|ui| for (value,label,key) in [(ReadFilter::All,"All","all"),(ReadFilter::Unread,"Unread","unread"),(ReadFilter::Read,"Read","read")] { let r=ui.selectable_value(&mut actions.view.read,value,label); target!(ui,r,&format!("read/{key}"),label,TargetKind::Selectable); if r.changed(){actions.view.page=0;} });
            ui.horizontal_wrapped(|ui| for (value,label,key) in [(DateFilter::All,"Any date","all"),(DateFilter::Day,"Last day","day"),(DateFilter::Week,"Last 7 days","week"),(DateFilter::Month,"Last 30 days","month")] { let r=ui.add_enabled(runtime.now_unix.is_some() || value==DateFilter::All,egui::Button::new(label).selected(actions.view.date==value)); target!(ui,r,&format!("date/{key}"),label,TargetKind::Selectable); if r.clicked(){actions.view.date=value;actions.view.page=0;} });
            ui.label("Source");
            ui.horizontal_wrapped(|ui| {
                let r=ui.selectable_value(&mut actions.view.source,None,"All sources"); target!(ui,r,"source/all","All sources",TargetKind::Selectable); if r.changed(){actions.view.page=0;}
                for source in [Source::Launcher,Source::Timer,Source::Macro,Source::Activity,Source::Calendar,Source::HotFolder,Source::Transfer,Source::Windows] { let label=format!("{source:?}"); let r=ui.selectable_value(&mut actions.view.source,Some(source),&label); target!(ui,r,&format!("source/{source:?}"),label,TargetKind::Selectable); if r.changed(){actions.view.page=0;} }
            });
            ui.label("Category"); ui.horizontal_wrapped(|ui| {
                let r=ui.selectable_value(&mut actions.view.category,None,"All categories"); target!(ui,r,"category/all","All categories",TargetKind::Selectable); if r.changed(){actions.view.page=0;}
                for category in [Category::Reminder,Category::Completed,Category::Failed,Category::Warning,Category::Information] { let label=format!("{category:?}"); let r=ui.selectable_value(&mut actions.view.category,Some(category),&label); target!(ui,r,&format!("category/{category:?}"),label,TargetKind::Selectable); if r.changed(){actions.view.page=0;} }
            });
            let app_filter=ui.add(egui::TextEdit::singleline(&mut actions.view.app).hint_text("Filter Windows app").char_limit(256)); target!(ui,app_filter,"app-filter","Filter Windows app",TargetKind::TextEntry); if app_filter.changed(){actions.view.page=0;}
            ui.horizontal_wrapped(|ui| for (value,label,key) in [(Group::Date,"Date","date"),(Group::Source,"Source","source"),(Group::Category,"Category","category"),(Group::App,"App","app")] { let r=ui.selectable_value(&mut actions.view.group,value,label); target!(ui,r,&format!("group/{key}"),label,TargetKind::Selectable); if r.changed(){actions.view.page=0;} });
            let query=actions.view.search.to_lowercase(); let app_query=actions.view.app.to_lowercase();
            let mut rows:Vec<_>=records.iter().filter(|row| {
                (actions.view.read==ReadFilter::All || (actions.view.read==ReadFilter::Read)==row.read)
                    && match actions.view.date { DateFilter::All => true, filter => runtime.now_unix.and_then(|now| now.checked_sub(row.occurred_at)).is_some_and(|age| age >= 0 && age <= match filter { DateFilter::Day => 86_400, DateFilter::Week => 7*86_400, DateFilter::Month => 30*86_400, DateFilter::All => i64::MAX }) }
                    && actions.view.source.is_none_or(|source| row.id.source()==source)
                    && actions.view.category.is_none_or(|category| row.category==category)
                    && (app_query.is_empty() || app(row).is_some_and(|name| name.to_lowercase().contains(&app_query)))
                    && (query.is_empty() || summary(row).to_lowercase().contains(&query) || app(row).is_some_and(|name| name.to_lowercase().contains(&query)) || matches!(&row.content, Content::External { body: Some(body), .. } if body.to_lowercase().contains(&query)))
            }).map(|row|(group(row,actions.view.group),row)).collect();
            rows.sort_by(|(ga,a),(gb,b)| ga.cmp(gb).then_with(|| b.occurred_at.cmp(&a.occurred_at)).then_with(|| b.updated_at.cmp(&a.updated_at)));
            let pages=rows.len().div_ceil(50).max(1); actions.view.page=actions.view.page.min(pages-1);
            ui.horizontal(|ui| { let b=ui.add_enabled(actions.view.page>0,egui::Button::new("Previous page")); target!(ui,b,"page/previous","Previous page",TargetKind::Button); if b.clicked(){actions.view.page-=1;} ui.label(format!("{} notifications · page {} / {}",rows.len(),actions.view.page+1,pages)); let b=ui.add_enabled(actions.view.page+1<pages,egui::Button::new("Next page")); target!(ui,b,"page/next","Next page",TargetKind::Button); if b.clicked(){actions.view.page+=1;} });
            let mut previous=None;
            for (label,row) in rows.into_iter().skip(actions.view.page*50).take(50) {
                if previous.as_ref()!=Some(&label){ui.strong(&label);previous=Some(label);}
                let selected=actions.view.selected.as_ref()==Some(&row.id);
                let text=format!("{}{} · {}",if row.read {""} else {"● "},summary(row),date(row.occurred_at));
                let b=ui.push_id(&row.id,|ui| ui.selectable_label(selected,text)).inner;
                let key=format!("record/{:x}",egui::Id::new(&row.id).value()); target!(ui,b,&key,"Select notification",TargetKind::Selectable);
                if b.clicked(){actions.view.selected=Some(row.id.clone()); actions.view.clear=None;}
            }
            if let Some(record)=actions.view.selected.as_ref().and_then(|id| records.iter().find(|r| &r.id==id)) {
                ui.separator(); ui.heading(summary(record)); ui.label(format!("Source: {:?} · {:?} · {:?}",record.id.source(),record.category,record.status));
                ui.label(format!("Occurred: {} · Updated: {}",date(record.occurred_at),date(record.updated_at)));
                if let Some(name)=app(record){ui.label(format!("App: {name}"));}
                if let Content::External {body:Some(body),..}=&record.content {ui.label(body);}
                ui.horizontal_wrapped(|ui| {
                    for (label,key,intent) in [(if record.read {"Mark unread"} else {"Mark read"},"read-toggle",Intent::Read(!record.read)), (if record.pinned {"Unpin"} else {"Pin"},"pin-toggle",Intent::Pin(!record.pinned)), ("Dismiss locally","dismiss",Intent::Dismiss), ("Create Todo","todo",Intent::CreateTodo)] {
                        let b=ui.add_enabled(!runtime.busy,egui::Button::new(label)); target!(ui,b,key,label,TargetKind::Button); if b.clicked(){actions.intent=Some(intent);}
                    }
                    let b=ui.add_enabled(!runtime.busy && record.related.is_some() && record.status!=Status::SourceRemoved,egui::Button::new("Open related")); target!(ui,b,"related","Open related",TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::OpenRelated);}
                });
                if record.status!=Status::SourceRemoved && matches!(&record.related,Some(RelatedItem::Timer{..}|RelatedItem::Calendar{..})) {
                    ui.horizontal_wrapped(|ui| for minutes in [5,10,15] { let label=format!("Snooze {minutes} min"); let b=ui.add_enabled(!runtime.busy,egui::Button::new(&label)); target!(ui,b,&format!("snooze/{minutes}"),label,TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::Snooze(minutes));} });
                }
                ui.horizontal_wrapped(|ui| { let b=ui.button("Clear this source…"); target!(ui,b,"clear/source","Clear this source",TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::BeginClear(ClearTarget::Source(record.id.source())));} if let Some(name)=app(record) { let b=ui.button("Clear this app…"); target!(ui,b,"clear/app","Clear this app",TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::BeginClear(ClearTarget::App(name.to_owned())));} } });
            }
            let b=ui.button("Clear all locally…"); target!(ui,b,"clear/all","Clear all locally",TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::BeginClear(ClearTarget::All));}
            if let Some(clear)=&actions.view.clear { ui.group(|ui| {
                ui.label(match clear {ClearTarget::Source(_) => "Clear this source from the local Inbox?", ClearTarget::App(_) => "Clear this app from the local Inbox?", ClearTarget::All => "Clear all local Inbox records?"});
                ui.weak("This does not dismiss or delete notifications from Windows or their source.");
                let b=ui.add_enabled(!runtime.busy,egui::Button::new("Confirm local clear")); target!(ui,b,"clear/confirm","Confirm local clear",TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::ConfirmClear);}
                let b=ui.button("Cancel"); target!(ui,b,"clear/cancel","Cancel local clear",TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::CancelClear);}
            }); }
            ui.separator(); ui.heading("Inbox settings");
            if let Some(settings)=&mut actions.view.settings {
                ui.label("Pins prevent expiry but count toward the limit. New items may be omitted when pins fill it.");
                ui.horizontal(|ui| { ui.label("Keep days"); let r=ui.add(egui::DragValue::new(&mut settings.retention_days).range(1..=90)); target!(ui,r,"settings/days","Retention days",TargetKind::TextEntry); ui.label("Maximum records"); let r=ui.add(egui::DragValue::new(&mut settings.max_records).range(1..=5000)); target!(ui,r,"settings/count","Maximum records",TargetKind::TextEntry); });
                ui.weak("Mute suppresses presentation. Exclude prevents new ingestion; existing records remain.");
                for source in [Source::Launcher,Source::Timer,Source::Macro,Source::Activity,Source::Calendar,Source::HotFolder,Source::Transfer,Source::Windows] {
                    ui.horizontal(|ui| { ui.label(format!("{source:?}")); let mut muted=settings.muted.contains(&source); let b=ui.checkbox(&mut muted,"Mute unread indicator"); target!(ui,b,&format!("settings/{source:?}/mute"),format!("Mute {source:?}"),TargetKind::Checkbox); if b.changed(){if muted {settings.muted.insert(source);}else{settings.muted.remove(&source);}} let mut excluded=settings.excluded.contains(&source); let b=ui.checkbox(&mut excluded,"Exclude new"); target!(ui,b,&format!("settings/{source:?}/exclude"),format!("Exclude {source:?}"),TargetKind::Checkbox); if b.changed(){if excluded {settings.excluded.insert(source);}else{settings.excluded.remove(&source);}} });
                }
                if let Some(name)=actions.view.selected.as_ref().and_then(|id| records.iter().find(|r| &r.id==id)).and_then(app) {
                    ui.horizontal(|ui| { ui.label(format!("App: {name}")); let mut muted=settings.muted_apps.contains(name); let b=ui.checkbox(&mut muted,"Mute app unread indicator"); target!(ui,b,"settings/app/mute","Mute selected app",TargetKind::Checkbox); if b.changed(){if muted {settings.muted_apps.insert(name.to_owned());}else{settings.muted_apps.remove(name);}} let mut excluded=settings.excluded_apps.contains(name); let b=ui.checkbox(&mut excluded,"Exclude app"); target!(ui,b,"settings/app/exclude","Exclude selected app",TargetKind::Checkbox); if b.changed(){if excluded {settings.excluded_apps.insert(name.to_owned());}else{settings.excluded_apps.remove(name);}} });
                }
                ui.separator();
                ui.label("Windows notifications");
                ui.weak("Coverage includes current toasts exposed by Windows and changes observed during collection.");
                let enable = ui.checkbox(&mut settings.external_enabled, "Capture Windows notifications");
                target!(ui, enable, "settings/windows/capture", "Capture Windows notifications", TargetKind::Checkbox);
                let text = ui.checkbox(&mut settings.retain_external_text, "Keep notification title and body in this session");
                target!(ui, text, "settings/windows/text", "Keep notification title and body in this session", TargetKind::Checkbox);
                let persist = ui.checkbox(&mut settings.persist_external, "Keep Windows records across restarts");
                target!(ui, persist, "settings/windows/persist", "Keep Windows records across restarts", TargetKind::Checkbox);
                if settings.retain_external_text && settings.persist_external {
                    ui.weak("Windows notification text can be saved to this profile.");
                }
                if matches!(runtime.windows_access, WindowsAccess::Off | WindowsAccess::SetupRequired | WindowsAccess::Unsupported) {
                    ui.weak("Windows capture setup is documented in docs/notification-inbox-windows.md.");
                }
                if runtime.windows_access == WindowsAccess::Denied {
                    ui.weak("You can review notification access in Windows privacy settings.");
                }
                let can_request = settings.external_enabled && snapshot.inbox.settings().external_enabled
                    && matches!(runtime.windows_access, WindowsAccess::PermissionNotGranted | WindowsAccess::Denied);
                let request = ui.add_enabled(!runtime.busy && can_request, egui::Button::new("Request Windows access"));
                target!(ui, request, "settings/windows/request", "Request Windows access", TargetKind::Button);
                if request.clicked() {
                    actions.intent = Some(Intent::RequestWindowsAccess);
                }
                let b=ui.add_enabled(!runtime.busy && settings.valid(),egui::Button::new("Apply Inbox settings")); target!(ui,b,"settings/apply","Apply Inbox settings",TargetKind::Button); if b.clicked(){actions.intent=Some(Intent::ApplySettings);}
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_inbox_dialog_close_retires_session() {
        let mut dialog = NotificationInboxDialog::default();
        dialog.open();
        let old = dialog.session();
        dialog.close();
        dialog.open();
        assert_ne!(old, dialog.session());
    }
}
