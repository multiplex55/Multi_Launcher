//! Typed author-editable step fields. IDs, numeric values, enum choices and
//! migration payloads are deliberately outside this boundary.
use super::{DiagnosticSeverity, SearchRegion, model::*, variables::*};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Template,
    VariableRead,
    VariableWrite,
    Image,
    Color,
    KeyCharacter,
}

/// Typed paths distinguish repeated and nested fields without serialized keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldPart {
    Label,
    Comment,
    Text,
    Title,
    TitleRegex,
    Description,
    Process,
    Class,
    Sound,
    Program,
    WorkingDirectory,
    Query,
    Variable,
    Value,
    DefaultValue,
    Prompt,
    Image,
    Color,
    Path,
    PathOutput,
    Found,
    MatchedText,
    MatchCount,
    Point,
    X,
    Y,
    Window,
    Selector,
    AutomationId,
    Name,
    ClassName,
    FrameworkId,
    ControlType,
    Target,
    From,
    To,
    Region,
    Language,
    Condition,
    Not,
    Outputs,
    Argument(usize),
    CallArgument(usize),
    CallOutput(usize),
    ReturnOutput(usize),
    Child(usize),
    Ancestor(usize),
    Key(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct FieldPath(pub Vec<FieldPart>);

impl FieldPath {
    fn child(&self, part: FieldPart) -> Self {
        let mut path = self.0.clone();
        path.push(part);
        Self(path)
    }
}

impl std::fmt::Display for FieldPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, part) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, " / ")?;
            }
            match part {
                FieldPart::Argument(i) => write!(f, "Argument {}", i + 1)?,
                FieldPart::CallArgument(i) => write!(f, "Call argument {}", i + 1)?,
                FieldPart::CallOutput(i) => write!(f, "Call output {}", i + 1)?,
                FieldPart::ReturnOutput(i) => write!(f, "Return output {}", i + 1)?,
                FieldPart::Child(i) => write!(f, "Condition {}", i + 1)?,
                FieldPart::Ancestor(i) => write!(f, "Ancestor {}", i + 1)?,
                FieldPart::Key(i) => write!(f, "Key {}", i + 1)?,
                part => write!(
                    f,
                    "{}",
                    match part {
                        FieldPart::Label => "Label",
                        FieldPart::Comment => "Comment",
                        FieldPart::Text => "Text",
                        FieldPart::Title => "Title",
                        FieldPart::TitleRegex => "Title regex",
                        FieldPart::Description => "Description",
                        FieldPart::Process => "Process",
                        FieldPart::Class => "Class",
                        FieldPart::Sound => "Sound",
                        FieldPart::Program => "Program",
                        FieldPart::WorkingDirectory => "Working directory",
                        FieldPart::Query => "Launcher query",
                        FieldPart::Variable => "Variable",
                        FieldPart::Value => "Value",
                        FieldPart::DefaultValue => "Default value",
                        FieldPart::Prompt => "Prompt",
                        FieldPart::Image => "Image filename",
                        FieldPart::Color => "Color",
                        FieldPart::Path => "Path",
                        FieldPart::PathOutput => "Path output",
                        FieldPart::Found => "Found",
                        FieldPart::MatchedText => "Matched text",
                        FieldPart::MatchCount => "Match count",
                        FieldPart::Point => "Point",
                        FieldPart::X => "X",
                        FieldPart::Y => "Y",
                        FieldPart::Window => "Window",
                        FieldPart::Selector => "Selector",
                        FieldPart::AutomationId => "Automation ID",
                        FieldPart::Name => "Name",
                        FieldPart::ClassName => "Class name",
                        FieldPart::FrameworkId => "Framework ID",
                        FieldPart::ControlType => "Custom control type",
                        FieldPart::Target => "Target",
                        FieldPart::From => "From",
                        FieldPart::To => "To",
                        FieldPart::Region => "Region",
                        FieldPart::Language => "Language",
                        FieldPart::Condition => "Condition",
                        FieldPart::Not => "Not",
                        FieldPart::Outputs => "Outputs",
                        _ => unreachable!("indexed field paths were handled above"),
                    }
                )?,
            }
        }
        Ok(())
    }
}

enum Storage<'a> {
    String(&'a mut String),
    Image(&'a mut MkImageRef),
}

/// The only write access offered by the visitor validates the field's type.
pub struct EditableField<'a> {
    kind: FieldKind,
    storage: Storage<'a>,
}

impl EditableField<'_> {
    pub fn kind(&self) -> FieldKind {
        self.kind
    }
    pub fn value(&self) -> &str {
        match &self.storage {
            Storage::String(value) => value,
            Storage::Image(value) => value.filename(),
        }
    }
    pub fn replace(&mut self, value: String) -> Result<(), String> {
        if self.value() == value {
            return Ok(());
        }
        match self.kind {
            FieldKind::VariableRead if value.is_empty() => {
                return Err("Variable reference cannot be empty".into());
            }
            FieldKind::VariableWrite => {
                validate_variable_name(&value).map_err(str::to_owned)?;
            }
            FieldKind::Template => validate_template(&value)?,
            FieldKind::Color => {
                super::parse_rgb(&value).map_err(|e| e.to_string())?;
            }
            FieldKind::KeyCharacter if value.len() != 1 || !value.is_ascii() => {
                return Err("A character key must be one ASCII character".into());
            }
            _ => {}
        }
        match &mut self.storage {
            Storage::String(target) => **target = value,
            Storage::Image(target) => **target = MkImageRef::new(value)?,
        }
        Ok(())
    }
}

fn validate_template(value: &str) -> Result<(), String> {
    // Runtime interpolation resolves exact keys, including historical Unicode
    // names; assignment-name rules must not narrow existing read semantics.
    super::validation::interpolation_syntax(value).map_err(str::to_owned)
}

type Visitor<'a> = dyn FnMut(FieldPath, EditableField<'_>) + 'a;

fn string(
    path: &FieldPath,
    part: FieldPart,
    value: &mut String,
    kind: FieldKind,
    visit: &mut Visitor<'_>,
) {
    visit(
        path.child(part),
        EditableField {
            kind,
            storage: Storage::String(value),
        },
    );
}
fn optional(
    path: &FieldPath,
    part: FieldPart,
    value: &mut Option<String>,
    kind: FieldKind,
    visit: &mut Visitor<'_>,
) {
    if let Some(value) = value {
        string(path, part, value, kind, visit);
    }
}
fn image(path: &FieldPath, value: &mut MkImageRef, visit: &mut Visitor<'_>) {
    visit(
        path.child(FieldPart::Image),
        EditableField {
            kind: FieldKind::Image,
            storage: Storage::Image(value),
        },
    );
}
fn matcher(path: &FieldPath, value: &mut MkWindowMatcher, visit: &mut Visitor<'_>) {
    use FieldPart::*;
    for (part, value) in [
        (Title, &mut value.title),
        (TitleRegex, &mut value.title_regex),
        (Process, &mut value.process),
        (Class, &mut value.class),
    ] {
        optional(path, part, value, FieldKind::Text, visit);
    }
}
fn region(path: &FieldPath, value: &mut SearchRegion, visit: &mut Visitor<'_>) {
    match value {
        SearchRegion::Window { matcher: value } | SearchRegion::ClientArea { matcher: value } => {
            matcher(path, value, visit)
        }
        SearchRegion::Desktop | SearchRegion::Monitor { .. } | SearchRegion::Rectangle { .. } => {}
    }
}
fn coordinate(path: &FieldPath, value: &mut MkCoordinateTarget, visit: &mut Visitor<'_>) {
    match value {
        MkCoordinateTarget::WindowClient { matcher: value, .. } => {
            matcher(&path.child(FieldPart::Window), value, visit)
        }
        MkCoordinateTarget::Variable { name } => string(
            path,
            FieldPart::Variable,
            name,
            FieldKind::VariableRead,
            visit,
        ),
        MkCoordinateTarget::Image { image: value, .. } => image(path, value, visit),
        MkCoordinateTarget::CurrentPosition
        | MkCoordinateTarget::Screen { .. }
        | MkCoordinateTarget::ActiveWindow { .. }
        | MkCoordinateTarget::Pixel { .. } => {}
    }
}
fn literal(path: &FieldPath, value: &mut MkValue, visit: &mut Visitor<'_>) {
    match value {
        MkValue::String(value) => string(path, FieldPart::Value, value, FieldKind::Text, visit),
        MkValue::Number(_) | MkValue::Boolean(_) | MkValue::Point(_) | MkValue::Null => {}
    }
}
fn source(path: &FieldPath, value: &mut MkValueSource, visit: &mut Visitor<'_>) {
    match value {
        // Reusable-call string sources interpolate in the active frame. This
        // role is distinct from SetVariable/condition literals, which stay raw.
        MkValueSource::Literal(MkValue::String(value)) => {
            string(path, FieldPart::Value, value, FieldKind::Template, visit)
        }
        MkValueSource::Literal(value) => literal(path, value, visit),
        MkValueSource::Variable { name } => string(
            path,
            FieldPart::Variable,
            name,
            FieldKind::VariableRead,
            visit,
        ),
    }
}
fn condition(path: &FieldPath, value: &mut MkCondition, visit: &mut Visitor<'_>) {
    use FieldPart::*;
    match value {
        MkCondition::Variable { name, value, .. } => {
            string(path, Variable, name, FieldKind::VariableRead, visit);
            literal(path, value, visit);
        }
        MkCondition::WindowExists { matcher: value }
        | MkCondition::WindowActive { matcher: value } => {
            matcher(&path.child(Window), value, visit)
        }
        MkCondition::ImageSearch { search, .. } => {
            image(path, &mut search.image, visit);
            region(&path.child(Region), &mut search.region, visit);
        }
        MkCondition::OcrTextSearch { search, .. } => {
            ocr_search(path, &mut search.search, visit);
        }
        MkCondition::PreviousImageResult { image: value, .. } => {
            if let Some(value) = value {
                image(path, value, visit);
            }
        }
        MkCondition::PixelResult { target, color, .. } => {
            coordinate(&path.child(Target), target, visit);
            string(path, Color, color, FieldKind::Color, visit);
        }
        MkCondition::All { conditions } | MkCondition::Any { conditions } => {
            for (i, value) in conditions.iter_mut().enumerate() {
                condition(&path.child(Child(i)), value, visit);
            }
        }
        MkCondition::Not { condition: value } => condition(&path.child(Not), value, visit),
    }
}
fn ocr_search(path: &FieldPath, value: &mut MkOcrSearchSpec, visit: &mut Visitor<'_>) {
    string(
        path,
        FieldPart::Text,
        &mut value.text,
        FieldKind::Template,
        visit,
    );
    if let MkOcrLanguage::LanguageTag(tag) = &mut value.language {
        string(path, FieldPart::Language, tag, FieldKind::Text, visit);
    }
    region(&path.child(FieldPart::Region), &mut value.region, visit);
}
fn outputs(path: &FieldPath, value: &mut MkImageOutputs, visit: &mut Visitor<'_>) {
    use FieldPart::*;
    for (part, value) in [
        (Found, &mut value.found),
        (Point, &mut value.point),
        (X, &mut value.x),
        (Y, &mut value.y),
    ] {
        optional(path, part, value, FieldKind::VariableWrite, visit);
    }
}
fn ocr_outputs(path: &FieldPath, value: &mut MkOcrOutputs, visit: &mut Visitor<'_>) {
    use FieldPart::*;
    for (part, value) in [
        (Found, &mut value.found),
        (MatchedText, &mut value.matched_text),
        (Point, &mut value.point),
        (X, &mut value.x),
        (Y, &mut value.y),
        (MatchCount, &mut value.match_count),
    ] {
        optional(path, part, value, FieldKind::VariableWrite, visit);
    }
}
fn control_type(path: &FieldPath, value: &mut Option<MkUiControlType>, visit: &mut Visitor<'_>) {
    if let Some(MkUiControlType::Other(value)) = value {
        string(path, FieldPart::ControlType, value, FieldKind::Text, visit);
    }
}
fn uia(path: &FieldPath, value: &mut MkUiPayload, visit: &mut Visitor<'_>) {
    use FieldPart::*;
    matcher(&path.child(Window), &mut value.window, visit);
    let path = path.child(Selector);
    let s = &mut value.selector;
    for (part, value) in [
        (AutomationId, &mut s.automation_id),
        (Name, &mut s.name),
        (ClassName, &mut s.class_name),
        (FrameworkId, &mut s.framework_id),
    ] {
        optional(&path, part, value, FieldKind::Text, visit);
    }
    control_type(&path, &mut s.control_type, visit);
    for (i, s) in s.ancestor_path.iter_mut().enumerate() {
        let path = path.child(Ancestor(i));
        for (part, value) in [
            (AutomationId, &mut s.automation_id),
            (Name, &mut s.name),
            (ClassName, &mut s.class_name),
            (FrameworkId, &mut s.framework_id),
        ] {
            optional(&path, part, value, FieldKind::Text, visit);
        }
        control_type(&path, &mut s.control_type, visit);
    }
}
fn key(path: &FieldPath, index: usize, value: &mut MkKey, visit: &mut Visitor<'_>) {
    if let MkKey::Character(value) = value {
        string(
            path,
            FieldPart::Key(index),
            value,
            FieldKind::KeyCharacter,
            visit,
        );
    }
}

/// Exhaustive action traversal includes dormant UIA and reusable-call fields;
/// visiting them does not change their separate runtime capability gates.
pub fn visit_step_fields(step: &mut MkStep, visit: &mut Visitor<'_>) {
    use FieldPart::*;
    let path = FieldPath::default();
    string(
        &path,
        Label,
        &mut step.metadata.label,
        FieldKind::Text,
        visit,
    );
    string(
        &path,
        Comment,
        &mut step.metadata.comment,
        FieldKind::Text,
        visit,
    );
    match &mut step.action {
        MkAction::CallMacro(call) => {
            for (i, binding) in call.arguments.iter_mut().enumerate() {
                source(&path.child(CallArgument(i)), &mut binding.source, visit);
            }
            for (i, binding) in call.outputs.iter_mut().enumerate() {
                string(
                    &path.child(CallOutput(i)),
                    Variable,
                    &mut binding.caller_variable,
                    FieldKind::VariableWrite,
                    visit,
                );
            }
        }
        MkAction::Return(ret) => {
            for (i, binding) in ret.outputs.iter_mut().enumerate() {
                source(&path.child(ReturnOutput(i)), &mut binding.source, visit);
            }
        }
        MkAction::KeyDown(value) | MkAction::KeyUp(value) | MkAction::KeyPress(value) => {
            key(&path, 0, value, visit)
        }
        MkAction::Hotkey(keys) => {
            for (i, value) in keys.iter_mut().enumerate() {
                key(&path, i, value, visit);
            }
        }
        MkAction::Text(value) => string(&path, Text, &mut value.text, FieldKind::Template, visit),
        MkAction::Notify(value) => {
            string(&path, Title, &mut value.title, FieldKind::Template, visit);
            string(
                &path,
                Description,
                &mut value.description,
                FieldKind::Template,
                visit,
            );
        }
        MkAction::PlaySound(value) => {
            string(&path, Sound, &mut value.sound, FieldKind::Text, visit)
        }
        MkAction::MouseMove(value) => coordinate(&path.child(Target), &mut value.target, visit),
        MkAction::MouseClick(value) => coordinate(&path.child(Target), &mut value.target, visit),
        MkAction::MouseDrag(value) => {
            coordinate(&path.child(From), &mut value.from, visit);
            coordinate(&path.child(To), &mut value.to, visit);
        }
        MkAction::Process(value) => {
            string(&path, Program, &mut value.program, FieldKind::Text, visit);
            for (i, value) in value.arguments.iter_mut().enumerate() {
                string(&path, Argument(i), value, FieldKind::Template, visit);
            }
            optional(
                &path,
                WorkingDirectory,
                &mut value.working_directory,
                FieldKind::Template,
                visit,
            );
        }
        MkAction::LauncherCommand(value) => {
            let old = value.query.clone();
            string(&path, Query, &mut value.query, FieldKind::Template, visit);
            if value.query != old {
                value.legacy_resolved_action = None;
            }
        }
        MkAction::WindowActivate(value) | MkAction::WindowWait(value) => {
            matcher(&path.child(Window), &mut value.matcher, visit)
        }
        MkAction::WindowClose(value) | MkAction::WindowState { matcher: value, .. } => {
            matcher(&path.child(Window), value, visit)
        }
        MkAction::WindowMoveResize(value) => {
            matcher(&path.child(Window), &mut value.matcher, visit)
        }
        MkAction::WaitUntil {
            condition: value, ..
        }
        | MkAction::If(value)
        | MkAction::WhileStart { condition: value } => {
            condition(&path.child(Condition), value, visit)
        }
        MkAction::SetVariable { name, value } => {
            string(&path, Variable, name, FieldKind::VariableWrite, visit);
            literal(&path, value, visit);
        }
        MkAction::UnsetVariable { name } => {
            string(&path, Variable, name, FieldKind::VariableWrite, visit)
        }
        MkAction::PromptInput(value) => {
            string(&path, Title, &mut value.title, FieldKind::Template, visit);
            string(&path, Prompt, &mut value.prompt, FieldKind::Template, visit);
            string(
                &path,
                DefaultValue,
                &mut value.default_value,
                FieldKind::Template,
                visit,
            );
            string(
                &path,
                Variable,
                &mut value.variable,
                FieldKind::VariableWrite,
                visit,
            );
        }
        MkAction::ImageFind(value) | MkAction::ImageClick(value) => {
            image(&path, &mut value.image, visit);
            region(&path.child(Region), &mut value.region, visit);
            outputs(&path.child(Outputs), &mut value.outputs, visit);
        }
        MkAction::OcrFindText(value) => {
            ocr_search(&path, &mut value.search, visit);
            ocr_outputs(&path.child(Outputs), &mut value.outputs, visit);
        }
        MkAction::OcrClickText(value) => ocr_search(&path, &mut value.search, visit),
        MkAction::OcrReadText(value) => {
            if let MkOcrLanguage::LanguageTag(tag) = &mut value.language {
                string(&path, Language, tag, FieldKind::Text, visit);
            }
            region(&path.child(Region), &mut value.region, visit);
            string(
                &path,
                Variable,
                &mut value.output_variable,
                FieldKind::VariableWrite,
                visit,
            );
        }
        MkAction::FindPixel(value) => {
            string(&path, Color, &mut value.color, FieldKind::Color, visit);
            region(&path.child(Region), &mut value.region, visit);
            outputs(&path.child(Outputs), &mut value.outputs, visit);
        }
        MkAction::CaptureScreenshot(value) => {
            region(&path.child(Region), &mut value.region, visit);
            optional(&path, Path, &mut value.path, FieldKind::Template, visit);
            optional(
                &path,
                PathOutput,
                &mut value.path_output,
                FieldKind::VariableWrite,
                visit,
            );
        }
        MkAction::WaitForVisualChange(value) => {
            region(&path.child(Region), &mut value.region, visit)
        }
        MkAction::PixelCheck { target, color, .. } => {
            coordinate(&path.child(Target), target, visit);
            string(&path, Color, color, FieldKind::Color, visit);
        }
        MkAction::UiInvoke(value)
        | MkAction::UiToggle(value)
        | MkAction::UiSelect(value)
        | MkAction::UiFocus(value)
        | MkAction::UiWait(value) => uia(&path, value, visit),
        MkAction::UiSetValue { target, value } => {
            uia(&path.child(Target), target, visit);
            string(&path, Value, value, FieldKind::Text, visit);
        }
        MkAction::UiReadValue { target, variable } => {
            uia(&path.child(Target), target, visit);
            string(&path, Variable, variable, FieldKind::VariableWrite, visit);
        }
        MkAction::ClickWithinRegion(_)
        | MkAction::MouseDown(_)
        | MkAction::MouseUp(_)
        | MkAction::MouseScroll { .. }
        | MkAction::Delay(_)
        | MkAction::VirtualDesktop(_)
        | MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatStart { .. }
        | MkAction::RepeatEnd
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringField {
    pub path: FieldPath,
    pub kind: FieldKind,
    pub value: String,
}

pub fn step_fields(step: &MkStep) -> Vec<AuthoringField> {
    let mut fields = Vec::new();
    visit_step_fields(&mut step.clone(), &mut |path, field| {
        fields.push(AuthoringField {
            path,
            kind: field.kind(),
            value: field.value().to_owned(),
        });
    });
    fields
}

/// Returns every persisted image reference in a step, including references in
/// nested conditions and coordinate targets. This shares the exhaustive typed
/// traversal used by authoring search/replace rather than inspecting JSON.
pub fn step_image_refs(step: &MkStep) -> BTreeSet<MkImageRef> {
    let mut images = BTreeSet::new();
    visit_step_fields(&mut step.clone(), &mut |_path, field| {
        if field.kind() == FieldKind::Image {
            images.insert(MkImageRef::from_filename(field.value()));
        }
    });
    images
}

/// Rewrites every typed image field using exact source filenames.
pub fn rewrite_step_image_refs(
    step: &mut MkStep,
    replacements: &HashMap<String, MkImageRef>,
) -> Result<(), String> {
    let mut error = None;
    visit_step_fields(step, &mut |_path, mut field| {
        if field.kind() != FieldKind::Image {
            return;
        }
        if let Some(replacement) = replacements.get(field.value()) {
            if let Err(reason) = field.replace(replacement.filename().to_owned()) {
                error = Some(reason);
            }
        }
    });
    error.map_or(Ok(()), Err)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldReplacement {
    pub step_id: u64,
    /// One-based source row, independent of folding.
    pub row: usize,
    pub field: FieldPath,
    pub old: String,
    pub new: String,
}

pub struct ReplacementPreview {
    baseline: MkMacroDocument,
    pub macro_id: u64,
    pub edits: Vec<FieldReplacement>,
}

impl ReplacementPreview {
    /// Exact, case-sensitive replacement of author-editable fields in one macro.
    pub fn prepare(
        document: &MkMacroDocument,
        macro_id: u64,
        query: &str,
        replacement: &str,
    ) -> Result<Self, String> {
        if query.is_empty() {
            return Err("Enter non-empty text to replace".into());
        }
        let owner = document
            .macros
            .iter()
            .find(|m| m.id == macro_id)
            .ok_or("The macro no longer exists")?;
        let mut edits = Vec::new();
        for (index, step) in owner.steps.iter().enumerate() {
            for field in step_fields(step) {
                let new = field.value.replace(query, replacement);
                if new != field.value {
                    edits.push(FieldReplacement {
                        step_id: step.id,
                        row: index + 1,
                        field: field.path,
                        old: field.value,
                        new,
                    });
                }
            }
        }
        Ok(Self {
            baseline: document.clone(),
            macro_id,
            edits,
        })
    }

    /// Prepare everything before publishing. Existing unrelated fatal errors
    /// remain visible but do not veto a harmless metadata/text edit.
    pub fn candidate(
        &self,
        document: &MkMacroDocument,
        current: Option<usize>,
    ) -> Result<MkMacroDocument, String> {
        if document != &self.baseline {
            return Err("The draft changed. Preview replacements again".into());
        }
        let edits: Vec<_> = match current {
            Some(index) => vec![
                self.edits
                    .get(index)
                    .ok_or("Select a replacement from the preview")?,
            ],
            None => self.edits.iter().collect(),
        };
        let mut candidate = document.clone();
        let owner = candidate
            .macros
            .iter_mut()
            .find(|m| m.id == self.macro_id)
            .ok_or("The macro no longer exists")?;
        for edit in edits {
            let step = owner
                .steps
                .get_mut(edit.row - 1)
                .filter(|s| s.id == edit.step_id)
                .ok_or("The step changed. Preview replacements again")?;
            let mut applied = false;
            let mut error = None;
            visit_step_fields(step, &mut |path, mut field| {
                if path == edit.field {
                    if field.value() != edit.old {
                        error = Some("The field changed. Preview replacements again".to_owned());
                    } else if let Err(reason) = field.replace(edit.new.clone()) {
                        error = Some(format!("Row {}, {}: {reason}", edit.row, path));
                    } else {
                        applied = true;
                    }
                }
            });
            if let Some(error) = error {
                return Err(error);
            }
            if !applied {
                return Err("The field no longer exists".into());
            }
        }
        let mut previous = super::validate_document(document, None);
        for diagnostic in super::validate_document(&candidate, None)
            .into_iter()
            .filter(|d| d.severity == DiagnosticSeverity::Fatal)
        {
            if let Some(index) = previous.iter().position(|old| old == &diagnostic) {
                previous.remove(index);
            } else {
                return Err(format!(
                    "Replacement would introduce an error: {}",
                    diagnostic.message
                ));
            }
        }
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            breakpoint: true,
            repeat: 42,
            delay_after_ms: 42,
            on_error: Default::default(),
            metadata: Default::default(),
            action,
        }
    }
    fn document(actions: Vec<MkAction>) -> MkMacroDocument {
        MkMacroDocument {
            macros: vec![MkMacro {
                id: 42,
                name: "Example".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                signature: Default::default(),
                steps: actions
                    .into_iter()
                    .enumerate()
                    .map(|(i, action)| step(i as u64 + 1, action))
                    .collect(),
            }],
            ..Default::default()
        }
    }
    fn window() -> MkWindowMatcher {
        MkWindowMatcher {
            title: Some("old".into()),
            title_regex: Some("old.*".into()),
            process: Some("old.exe".into()),
            class: Some("old_class".into()),
        }
    }
    fn output_names() -> MkImageOutputs {
        MkImageOutputs {
            found: Some("old_found".into()),
            point: Some("old_point".into()),
            x: Some("old_x".into()),
            y: Some("old_y".into()),
        }
    }
    fn window_region() -> SearchRegion {
        SearchRegion::Window { matcher: window() }
    }
    fn ui_payload() -> MkUiPayload {
        MkUiPayload {
            window: window(),
            wait: None,
            selector: MkUiSelector {
                automation_id: Some("old".into()),
                name: Some("old".into()),
                class_name: Some("old".into()),
                framework_id: Some("old".into()),
                control_type: Some(MkUiControlType::Other("old".into())),
                ancestor_path: vec![MkUiSelectorPart {
                    automation_id: Some("old".into()),
                    name: Some("old".into()),
                    class_name: Some("old".into()),
                    framework_id: Some("old".into()),
                    control_type: Some(MkUiControlType::Other("old".into())),
                }],
            },
        }
    }

    #[test]
    fn editable_field_families_are_typed_and_nested_paths_are_unique() {
        let cases = vec![
            (
                MkAction::Text(MkTextPayload {
                    text: "old ${old}".into(),
                    mode: MkTextMode::Type,
                }),
                1,
            ),
            (
                MkAction::Notify(MkNotifyPayload {
                    title: "old".into(),
                    description: "old ${old}".into(),
                    ..Default::default()
                }),
                2,
            ),
            (
                MkAction::PlaySound(MkPlaySoundPayload {
                    sound: "old.wav".into(),
                }),
                1,
            ),
            (
                MkAction::Process(MkProcessPayload {
                    program: "old.exe".into(),
                    arguments: vec!["old".into(), "${old}".into()],
                    working_directory: Some("old".into()),
                    wait: true,
                }),
                4,
            ),
            (
                MkAction::PromptInput(MkPromptInputPayload {
                    title: "old".into(),
                    prompt: "old".into(),
                    default_value: "${old}".into(),
                    variable: "old".into(),
                    copy_to_clipboard: true,
                }),
                4,
            ),
            (
                MkAction::MouseDrag(MkMouseDragPayload {
                    from: MkCoordinateTarget::WindowClient {
                        matcher: window(),
                        point: Default::default(),
                    },
                    to: MkCoordinateTarget::Image {
                        image: MkImageRef::new("old.png").unwrap(),
                        offset: Default::default(),
                    },
                    button: MkMouseButton::Left,
                    duration_ms: 42,
                }),
                5,
            ),
            (
                MkAction::CaptureScreenshot(MkScreenshotPayload {
                    region: window_region(),
                    destination: MkScreenshotDestination::File,
                    path: Some("old/${old}.png".into()),
                    format: MkScreenshotFormat::Png,
                    collision: MkFileCollisionPolicy::Unique,
                    path_output: Some("old".into()),
                }),
                6,
            ),
            (
                MkAction::UiReadValue {
                    target: ui_payload(),
                    variable: "old".into(),
                },
                15,
            ),
            (
                MkAction::ImageFind(MkImagePayload {
                    image: MkImageRef::new("old.png").unwrap(),
                    wait: Default::default(),
                    region: window_region(),
                    tolerance: 42,
                    alpha: super::super::AlphaPolicy::Compare,
                    return_point: super::super::ReturnPoint::Center,
                    not_found_policy: Default::default(),
                    outputs: output_names(),
                }),
                9,
            ),
            (
                MkAction::WaitUntil {
                    condition: MkCondition::All {
                        conditions: vec![
                            MkCondition::Not {
                                condition: Box::new(MkCondition::WindowExists {
                                    matcher: window(),
                                }),
                            },
                            MkCondition::ImageSearch {
                                search: MkImageSearchCondition {
                                    image: MkImageRef::new("old.png").unwrap(),
                                    region: window_region(),
                                    tolerance: 42,
                                    alpha: super::super::AlphaPolicy::Compare,
                                    return_point: super::super::ReturnPoint::Center,
                                },
                                found: true,
                            },
                            MkCondition::PreviousImageResult {
                                image: Some(MkImageRef::new("old.png").unwrap()),
                                found: true,
                            },
                            MkCondition::Variable {
                                name: "old".into(),
                                op: MkCompareOp::Eq,
                                value: MkValue::String("old".into()),
                            },
                            MkCondition::PixelResult {
                                target: MkCoordinateTarget::Variable { name: "old".into() },
                                color: "#010203".into(),
                                tolerance: 42,
                            },
                        ],
                    },
                    wait: Default::default(),
                },
                14,
            ),
        ];
        for (action, expected) in cases {
            let original = step(42, action);
            let fields = step_fields(&original);
            assert_eq!(
                fields.iter().filter(|f| !f.value.is_empty()).count(),
                expected,
                "{:?}",
                original.action
            );
            assert_eq!(
                fields
                    .iter()
                    .map(|f| &f.path)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                fields.len()
            );
            let mut edited = original.clone();
            visit_step_fields(&mut edited, &mut |_, mut field| {
                field.replace(field.value().replace("old", "new")).unwrap();
            });
            assert!(
                step_fields(&edited)
                    .iter()
                    .all(|field| !field.value.contains("old"))
            );
            assert_eq!(edited.id, original.id);
            assert_eq!(edited.repeat, original.repeat);
            assert_eq!(edited.delay_after_ms, original.delay_after_ms);
            assert_eq!(edited.breakpoint, original.breakpoint);
        }
    }

    #[test]
    fn sources_distinguish_reads_templates_and_literal_data() {
        let doc = document(vec![
            MkAction::SetVariable {
                name: "destination".into(),
                value: MkValue::String("${raw}".into()),
            },
            MkAction::CallMacro(MkCallMacroPayload {
                macro_id: 99,
                arguments: vec![
                    MkCallArgumentBinding {
                        parameter_id: MkSignatureId(42),
                        source: MkValueSource::Literal(MkValue::String("${old}".into())),
                    },
                    MkCallArgumentBinding {
                        parameter_id: MkSignatureId(43),
                        source: MkValueSource::Variable {
                            name: "mouse.x".into(),
                        },
                    },
                ],
                outputs: vec![MkCallOutputBinding {
                    output_id: MkSignatureId(44),
                    caller_variable: "destination".into(),
                }],
            }),
            MkAction::Return(MkReturnPayload {
                outputs: vec![MkReturnValueBinding {
                    output_id: MkSignatureId(45),
                    source: MkValueSource::Literal(MkValue::String("$${literal}".into())),
                }],
            }),
        ]);
        let roles = |index: usize| {
            step_fields(&doc.macros[0].steps[index])
                .into_iter()
                .filter(|f| !f.value.is_empty())
                .map(|f| f.kind)
                .collect::<Vec<_>>()
        };
        assert_eq!(roles(0), [FieldKind::VariableWrite, FieldKind::Text]);
        assert_eq!(
            roles(1),
            [
                FieldKind::Template,
                FieldKind::VariableRead,
                FieldKind::VariableWrite
            ]
        );
        assert_eq!(roles(2), [FieldKind::Template]);
        assert_eq!(
            step_fields(&doc.macros[0].steps[1])[2].path.to_string(),
            "Call argument 1 / Value"
        );
    }

    #[test]
    fn replacement_is_atomic_and_current_can_edit_around_unrelated_errors() {
        let mut doc = document(vec![
            MkAction::SetVariable {
                name: "old".into(),
                value: MkValue::Number(42.0),
            },
            MkAction::EndIf,
        ]);
        doc.macros[0].steps[0].metadata.label = "old".into();
        let original = doc.clone();
        let preview = ReplacementPreview::prepare(&doc, 42, "old", "mouse.x").unwrap();
        assert!(
            preview
                .candidate(&doc, None)
                .unwrap_err()
                .contains("read-only")
        );
        assert_eq!(doc, original);
        let current = preview.candidate(&doc, Some(0)).unwrap();
        assert_eq!(current.macros[0].steps[0].metadata.label, "mouse.x");
        assert_eq!(
            current.macros[0].steps[0].action,
            original.macros[0].steps[0].action
        );
        let mut changed = doc.clone();
        changed.macros[0].name = "Changed".into();
        assert!(
            preview
                .candidate(&changed, None)
                .unwrap_err()
                .contains("draft changed")
        );
        assert!(ReplacementPreview::prepare(&doc, 42, "", "value").is_err());
        assert!(
            ReplacementPreview::prepare(&doc, 42, "absent", "value")
                .unwrap()
                .edits
                .is_empty()
        );
    }

    #[test]
    fn replacements_validate_image_boundaries_and_new_semantic_errors() {
        let doc = document(vec![
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Image {
                    image: MkImageRef::new("old.png").unwrap(),
                    offset: Default::default(),
                },
                duration_ms: 0,
            }),
            MkAction::WindowClose(MkWindowMatcher {
                title_regex: Some("old".into()),
                ..Default::default()
            }),
        ]);
        let invalid = ReplacementPreview::prepare(&doc, 42, "old", "../new").unwrap();
        assert!(invalid.candidate(&doc, None).is_err());
        let regex = ReplacementPreview::prepare(&doc, 42, "old", "[").unwrap();
        assert!(
            regex
                .candidate(&doc, Some(1))
                .unwrap_err()
                .contains("introduce an error")
        );
        let valid = ReplacementPreview::prepare(&doc, 42, "old", "new")
            .unwrap()
            .candidate(&doc, None)
            .unwrap();
        assert!(
            matches!(&valid.macros[0].steps[0].action, MkAction::MouseMove(MkMouseMovePayload { target: MkCoordinateTarget::Image { image, .. }, .. }) if image.filename() == "new.png")
        );
    }

    #[test]
    fn replacements_preserve_exact_unicode_and_escaped_interpolation() {
        let doc = document(vec![MkAction::Text(MkTextPayload {
            text: "old ${名字} $${literal with spaces} ${mouse.x}".into(),
            mode: MkTextMode::Type,
        })]);
        let changed = ReplacementPreview::prepare(&doc, 42, "old", "new")
            .unwrap()
            .candidate(&doc, None)
            .unwrap();
        assert!(
            matches!(&changed.macros[0].steps[0].action, MkAction::Text(p) if p.text == "new ${名字} $${literal with spaces} ${mouse.x}")
        );
        assert!(
            ReplacementPreview::prepare(&doc, 42, "${mouse.x}", "${")
                .unwrap()
                .candidate(&doc, None)
                .is_err()
        );
    }

    #[test]
    fn launcher_query_changes_clear_legacy_payload_without_replacing_it() {
        let doc = document(vec![MkAction::LauncherCommand(MkLauncherCommandPayload {
            query: "old".into(),
            legacy_resolved_action: Some(crate::actions::Action {
                label: "legacy-only".into(),
                desc: String::new(),
                action: "legacy-only".into(),
                args: None,
            }),
        })]);
        assert!(
            ReplacementPreview::prepare(&doc, 42, "legacy-only", "new")
                .unwrap()
                .edits
                .is_empty()
        );
        let changed = ReplacementPreview::prepare(&doc, 42, "old", "new")
            .unwrap()
            .candidate(&doc, None)
            .unwrap();
        assert!(
            matches!(&changed.macros[0].steps[0].action, MkAction::LauncherCommand(p) if p.query == "new" && p.legacy_resolved_action.is_none())
        );
    }

    #[test]
    fn identifiers_numeric_values_and_enum_names_are_not_replaceable() {
        let doc = document(vec![
            MkAction::SetVariable {
                name: "count".into(),
                value: MkValue::Number(42.0),
            },
            MkAction::KeyPress(MkKey::Function(4)),
            MkAction::RepeatStart { count: 42 },
            MkAction::RepeatEnd,
        ]);
        for query in ["42", "repeat_start", "schema_version", "Function"] {
            assert!(
                ReplacementPreview::prepare(&doc, 42, query, "99")
                    .unwrap()
                    .edits
                    .is_empty()
            );
        }
        let unchanged = ReplacementPreview::prepare(&doc, 42, "count", "count")
            .unwrap()
            .candidate(&doc, None)
            .unwrap();
        assert_eq!(unchanged, doc);
    }
}
