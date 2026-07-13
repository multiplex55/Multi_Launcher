use crate::actions::clipboard;

pub fn tsv_row(fields: &[impl AsRef<str>]) -> String {
    fields
        .iter()
        .map(|f| f.as_ref().replace(['\t', '\n', '\r'], " "))
        .collect::<Vec<_>>()
        .join("\t")
}

pub fn set_clipboard_text(text: &str) -> anyhow::Result<()> {
    clipboard::set_text(text)
}
