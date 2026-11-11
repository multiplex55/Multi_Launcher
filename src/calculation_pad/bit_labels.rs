//! Bounded labels-only interchange for the programmer inspector.
use super::{InspectorAnnotation, inspector::validate_annotations};
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

const LIMIT: usize = 128 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelFile {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_width: Option<u32>,
    labels: Vec<LabelEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelEntry {
    label: String,
    start: u32,
    end_exclusive: u32,
}

fn decode(bytes: &[u8]) -> anyhow::Result<Vec<InspectorAnnotation>> {
    if bytes.len() > LIMIT {
        bail!("Bit-label file exceeds 128 KiB");
    }
    let file: LabelFile = serde_json::from_slice(bytes).context("Invalid bit-label JSON")?;
    if file.version != 1 {
        bail!("Unsupported bit-label version {}", file.version);
    }
    let labels = file
        .labels
        .into_iter()
        .map(|entry| InspectorAnnotation {
            range: entry.start..entry.end_exclusive,
            label: entry.label,
        })
        .collect::<Vec<_>>();
    validate_annotations(&labels)
        .map_err(|error| anyhow::anyhow!("Invalid bit labels: {error:?}"))?;
    Ok(labels)
}

pub(crate) fn read_bit_labels(path: &Path) -> anyhow::Result<Vec<InspectorAnnotation>> {
    let metadata = fs::symlink_metadata(path).context("Inspect bit-label file")?;
    if !metadata.file_type().is_file() {
        bail!("Bit-label path is not a regular file");
    }
    let file = File::open(path).context("Open bit-label file")?;
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .context("Read bit-label file")?;
    decode(&bytes)
}

pub(crate) fn write_bit_labels(
    path: &Path,
    labels: &[InspectorAnnotation],
    source_width: u32,
) -> anyhow::Result<()> {
    validate_annotations(labels)
        .map_err(|error| anyhow::anyhow!("Invalid bit labels: {error:?}"))?;
    let file = LabelFile {
        version: 1,
        source_width: Some(source_width),
        labels: labels
            .iter()
            .map(|entry| LabelEntry {
                label: entry.label.clone(),
                start: entry.range.start,
                end_exclusive: entry.range.end,
            })
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&file).context("Encode bit-label JSON")?;
    if bytes.len() > LIMIT {
        bail!("Bit-label file exceeds 128 KiB");
    }
    crate::common::atomic_file::save_atomic(path, &bytes).context("Save bit-label file")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_labels_roundtrip_order_unicode_overlap_and_out_of_width() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("labels.json");
        let labels = vec![
            InspectorAnnotation {
                range: 0..8,
                label: "状态".into(),
            },
            InspectorAnnotation {
                range: 4..12,
                label: String::new(),
            },
            InspectorAnnotation {
                range: 100..128,
                label: "high".into(),
            },
        ];
        write_bit_labels(&path, &labels, 8).unwrap();
        assert_eq!(read_bit_labels(&path).unwrap(), labels);
        write_bit_labels(&path, &[], 128).unwrap();
        assert!(read_bit_labels(&path).unwrap().is_empty());
    }

    #[test]
    fn bit_labels_reject_invalid_content_and_preserve_destination() {
        for bytes in [
            b"{\"version\":2,\"labels\":[]}".as_slice(),
            b"{\"version\":1,\"labels\":[{\"label\":\"x\",\"start\":-1,\"end_exclusive\":2}]}".as_slice(),
            b"{\"version\":1,\"labels\":[{\"label\":\"x\",\"start\":0.5,\"end_exclusive\":2}]}".as_slice(),
            b"{\"version\":1,\"labels\":[{\"label\":\"x\",\"start\":1,\"end_exclusive\":1}]}".as_slice(),
            b"{\"version\":1,\"labels\":[{\"label\":\"x\",\"start\":0,\"end_exclusive\":129}]}".as_slice(),
            b"{\"version\":1,\"labels\":[{\"label\":\"x\",\"start\":0,\"end_exclusive\":1},{\"label\":\"y\",\"start\":0,\"end_exclusive\":1}]}".as_slice(),
            b"\xff".as_slice(),
        ] { assert!(decode(bytes).is_err(), "{bytes:?}"); }
        let too_long = vec![InspectorAnnotation {
            range: 0..1,
            label: "界".repeat(43),
        }];
        let too_many = (0..129)
            .map(|_| InspectorAnnotation {
                range: 0..1,
                label: String::new(),
            })
            .collect::<Vec<_>>();
        assert!(validate_annotations(&too_long).is_err());
        assert!(validate_annotations(&too_many).is_err());
        let many_json = serde_json::json!({"version": 1, "labels": (0..129).map(|_| serde_json::json!({"label":"x","start":0,"end_exclusive":1})).collect::<Vec<_>>()});
        assert!(decode(serde_json::to_string(&many_json).unwrap().as_bytes()).is_err());
        let long_json = serde_json::json!({"version":1,"labels":[{"label":"界".repeat(43),"start":0,"end_exclusive":1}]});
        assert!(decode(serde_json::to_string(&long_json).unwrap().as_bytes()).is_err());
        let nested = format!(
            "{{\"version\":1,\"labels\":{}0{}}}",
            "[".repeat(150),
            "]".repeat(150)
        );
        assert!(decode(nested.as_bytes()).is_err());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("labels.json");
        std::fs::write(&path, b"original").unwrap();
        assert!(write_bit_labels(&path, &too_long, 8).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        std::fs::write(&path, vec![b' '; LIMIT + 1]).unwrap();
        assert!(read_bit_labels(&path).is_err());
        assert!(read_bit_labels(temp.path()).is_err());
        assert!(read_bit_labels(&temp.path().join("missing")).is_err());
        let protected = temp.path().join("protected-directory");
        std::fs::create_dir(&protected).unwrap();
        std::fs::write(protected.join("sentinel"), b"untouched").unwrap();
        assert!(write_bit_labels(&protected, &[], 8).is_err());
        assert_eq!(
            std::fs::read(protected.join("sentinel")).unwrap(),
            b"untouched"
        );
    }
}
