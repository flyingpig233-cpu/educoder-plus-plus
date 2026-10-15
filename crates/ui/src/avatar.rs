use crate::Controller;
use educoder_core::models::User;
use serde::{Deserialize, Serialize};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{
    io::Cursor,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
struct CachedAvatar {
    path: String,
    bytes: Vec<u8>,
}

impl Controller {
    pub(super) fn load_avatar(self: &Arc<Self>, user: User, online: bool) {
        let token = self.avatar_version.fetch_add(1, Ordering::SeqCst) + 1;
        let (client, cache) = {
            let state = self.state.lock().expect("UI state");
            (state.client.clone(), state.cache.clone())
        };
        let (Some(client), Some(path)) = (client, user.avatar_path.clone()) else {
            if let Some(app) = self.ui.upgrade() {
                app.set_user_avatar(Image::default());
            }
            return;
        };
        let c = self.clone();
        let ui = self.ui.clone();
        self.runtime.spawn(async move {
            if let Some(store) = &cache
                && let Some(cached) = store
                    .get::<CachedAvatar>("avatar", Duration::from_secs(7 * 86400))
                    .await
                && cached.value.path == path
            {
                let fresh = !cached.stale;
                if let Ok(Some(pixels)) =
                    tokio::task::spawn_blocking(move || decode_avatar(&cached.value.bytes)).await
                {
                    let c2 = c.clone();
                    let _ = ui.upgrade_in_event_loop(move |app| {
                        if c2.avatar_version.load(Ordering::SeqCst) == token {
                            app.set_user_avatar(Image::from_rgba8(pixels));
                        }
                    });
                    if fresh {
                        return;
                    }
                }
            }
            if !online {
                return;
            }
            let Ok(Some(bytes)) = client.get_user_avatar(&user).await else {
                return;
            };
            let decoded = bytes.clone();
            let Ok(Some(pixels)) =
                tokio::task::spawn_blocking(move || decode_avatar(&decoded)).await
            else {
                return;
            };
            if let Some(store) = cache {
                let _ = store.put("avatar", &CachedAvatar { path, bytes }).await;
            }
            let _ = ui.upgrade_in_event_loop(move |app| {
                if c.avatar_version.load(Ordering::SeqCst) == token {
                    app.set_user_avatar(Image::from_rgba8(pixels));
                }
            });
        });
    }
}

fn decode_avatar(bytes: &[u8]) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let mut pixels = reader
        .decode()
        .ok()?
        .resize_to_fill(96, 96, image::imageops::FilterType::Triangle)
        .to_rgba8();
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        let distance = ((x as f32 + 0.5 - 48.0).powi(2) + (y as f32 + 0.5 - 48.0).powi(2)).sqrt();
        pixel[3] = (pixel[3] as f32 * (48.0 - distance).clamp(0.0, 1.0)) as u8;
    }
    Some(SharedPixelBuffer::clone_from_slice(pixels.as_raw(), 96, 96))
}
