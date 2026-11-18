//! Shared retained preview worker for the current Markdown draft.
use crate::notes_markdown::print_layout::{
    self, PaginatedDocument, PrintSettings,
    raster::{RasterPage, raster_page},
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AssetBase {
    Note(PathBuf),
    Studio(Option<PathBuf>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DocumentStamp {
    Note {
        panel: u64,
        host: super::utility_host::HostIdentity,
        slug: String,
        path: PathBuf,
        revision: u64,
    },
    Studio {
        epoch: u64,
        host: super::utility_host::HostIdentity,
        path: Option<PathBuf>,
        revision: Option<u64>,
    },
}
#[derive(Clone)]
pub(super) struct Snapshot {
    pub stamp: DocumentStamp,
    pub source: Arc<str>,
    pub base: AssetBase,
    pub settings: PrintSettings,
    pub date: String,
}
pub(super) struct SceneReceipt {
    pub stamp: DocumentStamp,
    pub settings: PrintSettings,
    pub date: String,
    pub source: Arc<str>,
    pub base: AssetBase,
    pub scene: Arc<PaginatedDocument>,
}
#[derive(Clone)]
enum Work {
    Prepare(Snapshot),
    Raster(Arc<SceneReceipt>),
}
struct Request {
    serial: u64,
    work: Work,
    page: usize,
    scale: f32,
    viewport: egui::ViewportId,
    ctx: egui::Context,
}
pub(super) struct Ready {
    pub serial: u64,
    pub receipt: Arc<SceneReceipt>,
    pub page: usize,
    pub scale: f32,
    pub raster: RasterPage,
}
pub(super) struct Reply {
    pub serial: u64,
    pub result: Result<Ready, String>,
}
#[derive(Default)]
struct Queue {
    request: Option<Request>,
    result: Option<Reply>,
    stopped: bool,
}
struct Shared {
    serial: AtomicU64,
    stopped: AtomicBool,
    queue: Mutex<Queue>,
    wake: Condvar,
}
pub(super) struct PreviewWorker {
    shared: Arc<Shared>,
}
impl Default for PreviewWorker {
    fn default() -> Self {
        let shared = Arc::new(Shared {
            serial: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        if std::thread::Builder::new()
            .name("markdown-print-preview".into())
            .spawn(move || run(worker))
            .is_err()
        {
            shared.stopped.store(true, Ordering::Release);
        }
        Self { shared }
    }
}
impl PreviewWorker {
    fn submit(
        &self,
        work: Work,
        page: usize,
        scale: f32,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) -> u64 {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        let serial = self
            .shared
            .serial
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        if self.shared.stopped.load(Ordering::Acquire) {
            queue.result = Some(Reply {
                serial,
                result: Err("Preview worker could not be started.".into()),
            });
            ctx.request_repaint_of(viewport);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
            return serial;
        }
        queue.result = None;
        queue.request = Some(Request {
            serial,
            work,
            page,
            scale,
            viewport,
            ctx: ctx.clone(),
        });
        drop(queue);
        self.shared.wake.notify_one();
        serial
    }
    pub fn prepare(
        &self,
        snapshot: Snapshot,
        page: usize,
        scale: f32,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) -> u64 {
        self.submit(Work::Prepare(snapshot), page, scale, ctx, viewport)
    }
    pub fn raster(
        &self,
        receipt: Arc<SceneReceipt>,
        page: usize,
        scale: f32,
        ctx: &egui::Context,
        viewport: egui::ViewportId,
    ) -> u64 {
        self.submit(Work::Raster(receipt), page, scale, ctx, viewport)
    }
    pub fn retire(&self) {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.serial.fetch_add(1, Ordering::AcqRel);
        queue.request = None;
        queue.result = None;
    }
    pub fn take(&self) -> Option<Reply> {
        self.shared
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .result
            .take()
    }
}
impl Drop for PreviewWorker {
    fn drop(&mut self) {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.serial.fetch_add(1, Ordering::AcqRel);
        queue.stopped = true;
        queue.request = None;
        drop(queue);
        self.shared.wake.notify_all();
    }
}
fn run(shared: Arc<Shared>) {
    loop {
        let request = {
            let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            while !q.stopped && q.request.is_none() {
                q = shared.wake.wait(q).unwrap_or_else(|e| e.into_inner());
            }
            if q.stopped {
                return;
            }
            q.request.take().unwrap()
        };
        let cancelled = || {
            shared.stopped.load(Ordering::Acquire)
                || shared.serial.load(Ordering::Acquire) != request.serial
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let receipt = match request.work {
                Work::Prepare(snapshot) => {
                    if snapshot.source.len() > print_layout::MAX_TEXT_BYTES {
                        return Err("Markdown source exceeds the print size limit.".into());
                    }
                    snapshot.settings.geometry().map_err(str::to_owned)?;
                    let resolver: Box<dyn crate::notes_markdown::LocalRasterResolver> =
                        match &snapshot.base {
                            AssetBase::Note(root) => {
                                Box::new(super::markdown_export::NoteRasterAssets::load_for_print(
                                    &snapshot.source,
                                    root,
                                    &cancelled,
                                ))
                            }
                            AssetBase::Studio(base) => {
                                Box::new(super::markdown_studio::LocalAssets::load_for_print(
                                    base.as_deref(),
                                    &snapshot.source,
                                    &cancelled,
                                ))
                            }
                        };
                    if cancelled() {
                        return Err("Preview cancelled.".into());
                    }
                    let scene = Arc::new(
                        print_layout::paginate_with_assets(
                            &snapshot.source,
                            &snapshot.settings,
                            &snapshot.date,
                            Some(resolver.as_ref()),
                            &cancelled,
                        )
                        .map_err(|e| e.to_string())?,
                    );
                    Arc::new(SceneReceipt {
                        stamp: snapshot.stamp,
                        settings: snapshot.settings,
                        date: snapshot.date,
                        source: snapshot.source,
                        base: snapshot.base,
                        scene,
                    })
                }
                Work::Raster(receipt) => receipt,
            };
            if cancelled() {
                return Err("Preview cancelled.".into());
            }
            let page = request
                .page
                .min(receipt.scene.pages.len().saturating_sub(1));
            let raster = raster_page(&receipt.scene, page, request.scale, &cancelled)
                .map_err(|e| e.to_string())?;
            Ok(Ready {
                serial: request.serial,
                receipt,
                page,
                scale: request.scale,
                raster,
            })
        }))
        .unwrap_or_else(|_| Err("Preview worker failed while preparing the document.".into()));
        if !cancelled() {
            let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            if !q.stopped && shared.serial.load(Ordering::Acquire) == request.serial {
                q.result = Some(Reply {
                    serial: request.serial,
                    result,
                });
                drop(q);
                request.ctx.request_repaint_of(request.viewport);
                request.ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        }
    }
}

/// Holds the authorized local root and opens assets relative to its handle.
#[cfg(windows)]
pub(super) struct PrintAssetRoot {
    held: crate::archive::print_publication::Directory,
}
#[cfg(windows)]
impl PrintAssetRoot {
    pub fn open(root: &std::path::Path) -> std::io::Result<Self> {
        let root = std::path::absolute(root)?;
        let held = crate::archive::print_publication::Directory::open(&root, false)?;
        Ok(Self { held })
    }
    pub fn read(
        &self,
        relative: &std::path::Path,
        limit: usize,
        cancel: &mut impl FnMut() -> bool,
    ) -> PrintRead {
        use std::io::Read;
        let fail = || PrintRead {
            bytes: Vec::new(),
            attempted: 0,
            complete: false,
        };
        if cancel() {
            return fail();
        }
        let Ok(mut file) = self.held.read_relative_file(relative) else {
            return fail();
        };
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut complete = false;
        while bytes.len() < limit {
            if cancel() {
                break;
            }
            let count = (limit - bytes.len()).min(buffer.len());
            match file.read(&mut buffer[..count]) {
                Ok(0) => {
                    complete = true;
                    break;
                }
                Ok(n) => bytes.extend_from_slice(&buffer[..n]),
                Err(_) => break,
            }
        }
        let attempted = bytes.len();
        PrintRead {
            bytes,
            attempted,
            complete,
        }
    }
}
#[cfg(not(windows))]
pub(super) struct PrintAssetRoot;
#[cfg(not(windows))]
impl PrintAssetRoot {
    pub fn open(_: &std::path::Path) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Local print assets require a native no-follow handle.",
        ))
    }
    pub fn read(&self, _: &std::path::Path, _: usize, _: &mut impl FnMut() -> bool) -> PrintRead {
        PrintRead {
            bytes: Vec::new(),
            attempted: 0,
            complete: false,
        }
    }
}
pub(super) struct PrintRead {
    pub bytes: Vec<u8>,
    pub attempted: usize,
    pub complete: bool,
}

#[cfg(test)]
mod tests {
    use super::super::utility_host::{HostIdentity, UtilityPresentation};
    use super::*;
    use std::time::{Duration, Instant};
    fn snapshot(source: &str, base: AssetBase, revision: u64) -> Snapshot {
        Snapshot {
            stamp: DocumentStamp::Note {
                panel: 7,
                host: HostIdentity {
                    host: UtilityPresentation::Docked,
                    generation: 1,
                },
                slug: "draft".into(),
                path: PathBuf::from("draft.md"),
                revision,
            },
            source: Arc::from(source),
            base,
            settings: PrintSettings::default(),
            date: "2026-11-18".into(),
        }
    }
    fn studio_snapshot(source: &str, base: AssetBase, revision: u64) -> Snapshot {
        let mut snapshot = snapshot(source, base, revision);
        snapshot.stamp = DocumentStamp::Studio {
            epoch: 3,
            host: HostIdentity {
                host: UtilityPresentation::Popout,
                generation: 2,
            },
            path: Some(PathBuf::from("studio.md")),
            revision: Some(revision),
        };
        snapshot
    }
    fn wait(worker: &PreviewWorker, serial: u64) -> Ready {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(reply) = worker.take() {
                assert_eq!(reply.serial, serial);
                return reply.result.unwrap();
            }
            assert!(Instant::now() < deadline, "print worker did not complete");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn text(scene: &PaginatedDocument) -> String {
        scene
            .pages
            .iter()
            .flat_map(|p| &p.drawables)
            .filter_map(|d| match d {
                print_layout::Drawable::Text { text, .. } => Some(text.as_ref()),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn markdown_print_worker_keeps_dirty_snapshot_and_reuses_scene_for_page_zoom() {
        let root = tempfile::tempdir().unwrap();
        let unused = root.path().join("assets");
        let worker = PreviewWorker::default();
        let ctx = egui::Context::default();
        let source = "# Draft\n\nunsaved text\n\n<!-- pagebreak -->\n\nsecond page";
        let request = snapshot(source, AssetBase::Note(unused.clone()), 9);
        let stamp = request.stamp.clone();
        let serial = worker.prepare(request, 0, 1.0, &ctx, egui::ViewportId::ROOT);
        let ready = wait(&worker, serial);
        assert_eq!(ready.receipt.stamp, stamp);
        assert_eq!(ready.receipt.source.as_ref(), source);
        assert!(text(&ready.receipt.scene).contains("unsaved text"));
        assert_eq!(ready.receipt.scene.pages.len(), 2);
        assert!(!unused.exists());
        let receipt = ready.receipt.clone();
        let serial = worker.raster(receipt.clone(), 1, 0.5, &ctx, egui::ViewportId::ROOT);
        let next = wait(&worker, serial);
        assert!(Arc::ptr_eq(&receipt, &next.receipt));
        assert_eq!(next.page, 1);
        assert_eq!(next.scale, 0.5);
        assert!(next.raster.width > 0);
    }
    #[test]
    fn markdown_print_worker_coalesces_latest_and_retires_stale_work() {
        let worker = PreviewWorker::default();
        let ctx = egui::Context::default();
        for revision in 0..8 {
            worker.prepare(
                snapshot(
                    &format!("draft {revision}"),
                    AssetBase::Studio(None),
                    revision,
                ),
                0,
                0.2,
                &ctx,
                egui::ViewportId::ROOT,
            );
        }
        let latest = studio_snapshot("latest dirty text", AssetBase::Studio(None), 99);
        let stamp = latest.stamp.clone();
        let serial = worker.prepare(latest, 0, 0.2, &ctx, egui::ViewportId::ROOT);
        let ready = wait(&worker, serial);
        assert_eq!(ready.receipt.stamp, stamp);
        assert!(text(&ready.receipt.scene).contains("latest dirty text"));
        worker.prepare(
            snapshot("retired", AssetBase::Studio(None), 100),
            0,
            1.0,
            &ctx,
            egui::ViewportId::ROOT,
        );
        worker.retire();
        std::thread::sleep(Duration::from_millis(100));
        assert!(worker.take().is_none());
        let serial = worker.prepare(
            snapshot("fresh", AssetBase::Studio(None), 101),
            0,
            0.2,
            &ctx,
            egui::ViewportId::ROOT,
        );
        assert!(text(&wait(&worker, serial).receipt.scene).contains("fresh"));
    }
    #[test]
    fn markdown_print_worker_preflights_before_asset_io() {
        let root = tempfile::tempdir().unwrap();
        let nonexistent = root.path().join("never-created");
        let ctx = egui::Context::default();
        let worker = PreviewWorker::default();
        let mut request = snapshot("draft", AssetBase::Note(nonexistent.clone()), 1);
        request.settings.body_size = f32::NAN;
        let serial = worker.prepare(request, 0, 0.2, &ctx, egui::ViewportId::ROOT);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(reply) = worker.take() {
                assert_eq!(reply.serial, serial);
                assert!(reply.result.is_err());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!nonexistent.exists());
        let oversized = "x".repeat(print_layout::MAX_TEXT_BYTES + 1);
        let request = snapshot(&oversized, AssetBase::Note(nonexistent.clone()), 2);
        let serial = worker.prepare(request, 0, 0.2, &ctx, egui::ViewportId::ROOT);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(reply) = worker.take() {
                assert_eq!(reply.serial, serial);
                assert!(reply.result.is_err());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!nonexistent.exists());
    }
    #[test]
    fn markdown_print_worker_slot_serial_and_retire_are_linearized() {
        let worker = Arc::new(PreviewWorker::default());
        let ctx = egui::Context::default();
        let a = worker.clone();
        let ac = ctx.clone();
        let first = std::thread::spawn(move || {
            for revision in 0..50 {
                a.prepare(
                    snapshot("first", AssetBase::Studio(None), revision),
                    0,
                    0.1,
                    &ac,
                    egui::ViewportId::ROOT,
                );
            }
        });
        let b = worker.clone();
        let bc = ctx.clone();
        let second = std::thread::spawn(move || {
            for revision in 0..50 {
                if revision % 2 == 0 {
                    b.retire();
                } else {
                    b.prepare(
                        snapshot("second", AssetBase::Studio(None), revision),
                        0,
                        0.1,
                        &bc,
                        egui::ViewportId::ROOT,
                    );
                }
            }
        });
        first.join().unwrap();
        second.join().unwrap();
        let queue = worker.shared.queue.lock().unwrap();
        let current = worker.shared.serial.load(Ordering::Acquire);
        if let Some(request) = &queue.request {
            assert_eq!(request.serial, current);
        }
        if let Some(reply) = &queue.result {
            assert_eq!(reply.serial, current);
        }
    }
    #[cfg(windows)]
    #[test]
    fn markdown_print_worker_rejects_final_reparse_asset() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("assets");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("outside.png");
        std::fs::write(&outside, b"secret").unwrap();
        let link = root.join("link.png");
        if std::os::windows::fs::symlink_file(&outside, &link).is_err() {
            return;
        }
        let held = PrintAssetRoot::open(&root).unwrap();
        let read = held.read(std::path::Path::new("link.png"), 100, &mut || false);
        assert!(!read.complete && read.bytes.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn markdown_print_worker_loads_local_assets_for_both_origins() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("assets");
        std::fs::create_dir(&assets).unwrap();
        let image = image_025::DynamicImage::ImageRgba8(image_025::ImageBuffer::from_pixel(
            2,
            2,
            image_025::Rgba([5, 10, 20, 255]),
        ));
        let mut png = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut png, image_025::ImageFormat::Png)
            .unwrap();
        std::fs::write(assets.join("p.png"), png.into_inner()).unwrap();
        let worker = PreviewWorker::default();
        let ctx = egui::Context::default();
        for base in [
            AssetBase::Note(assets.clone()),
            AssetBase::Studio(Some(dir.path().to_owned())),
        ] {
            let serial = worker.prepare(
                snapshot("![p](assets/p.png)", base, 1),
                0,
                0.2,
                &ctx,
                egui::ViewportId::ROOT,
            );
            let ready = wait(&worker, serial);
            assert_eq!(ready.receipt.scene.assets.len(), 1);
        }
    }
}
