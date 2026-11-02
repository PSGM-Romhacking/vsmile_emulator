//! Screenshots and recordings (GIF, MP4).

#[allow(unused_imports)]
use super::*;

impl App {
    /// Saves the current frame as PNG (4x, sharp pixels, no filter) to <Pictures>/<app name>.
    pub(super) fn screenshot(&mut self) {
        if !self.loaded {
            return;
        }
        let Some(dir) = crate::config::pictures_dir() else { return };
        let stem = self.cur_game.as_ref().and_then(|g| g.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| crate::brand::get().id.into());
        let path = dir.join(format!("{stem} {}.png", local_stamp()));
        let img = self.frame_image(4);
        match std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| img.save(&path).map_err(|e| e.to_string())) {
            Ok(()) => self.notify(trf("Screenshot saved: {file}", &[("file", &path.file_name().unwrap_or_default().to_string_lossy())])),
            Err(e) => self.notify(trf("Screenshot error: {error}", &[("error", &e)])),
        }
    }

    /// Toggles recording (GIF or MP4). Stopping only closes the input: the thread
    /// finishes the file and `poll_recordings` reports when it is ready.
    pub(super) fn toggle_record(&mut self, kind: crate::recorder::Kind) {
        use crate::recorder::{Kind, Recorder};
        if let Some(mut r) = self.recorder.take() {
            let same = r.kind == kind;
            r.stop();
            self.finishing.push(r);
            self.notify(tr("Finishing recording…"));
            if same {
                return;
            }
        }
        if !self.loaded {
            return;
        }
        if kind == Kind::Mp4 && !self.has_ffmpeg {
            self.notify(tr("Recording video needs ffmpeg installed"));
            return;
        }
        let stem = self.cur_game.as_ref().and_then(|g| g.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| crate::brand::get().id.into());
        let (dir, ext) = match kind {
            Kind::Gif => (crate::config::videos_dir(), "gif"),
            Kind::Mp4 => (crate::config::videos_dir(), "mp4"),
        };
        let Some(dir) = dir else { return };
        let v = self.core.video();
        let fmt = crate::recorder::VideoFormat { width: v.width, height: v.height, fps_num: self.core.fps().0, fps_den: self.core.fps().1 };
        match Recorder::start(kind, dir.join(format!("{stem} {}.{ext}", local_stamp())), self.audio.rate, fmt) {
            Ok(r) => {
                self.recorder = Some(r);
                self.notify(tr("Recording…"));
            }
            Err(e) => self.notify(trf("Recording error: {error}", &[("error", &e)])),
        }
    }

    /// Reports when a recording has finished encoding.
    pub(super) fn poll_recordings(&mut self) {
        let mut msgs = vec![];
        self.finishing.retain_mut(|r| match r.try_finish() {
            Some(Ok(p)) => {
                msgs.push(trf("Recording saved: {file}", &[("file", &p.file_name().unwrap_or_default().to_string_lossy())]));
                false
            }
            Some(Err(e)) => {
                msgs.push(trf("Recording error: {error}", &[("error", &e)]));
                false
            }
            None => true,
        });
        if let Some(m) = msgs.pop() {
            self.notify(m);
        }
    }
}
