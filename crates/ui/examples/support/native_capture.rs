//! Capture only the synthetic test window, never the desktop.
use slint::winit_030::winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
use windows_sys::Win32::Graphics::Gdi::*;
pub fn pixels(window: &Window, height: u32) -> Vec<u8> {
    let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
        panic!("not a Windows window")
    };
    let hwnd = handle.hwnd.get() as *mut _;
    let width = window.inner_size().width;
    unsafe {
        let dc = GetWindowDC(hwnd);
        let memory = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, width as i32, height as i32);
        let previous = SelectObject(memory, bitmap);
        assert_ne!(
            BitBlt(memory, 0, 0, width as i32, height as i32, dc, 0, 0, SRCCOPY),
            0
        );
        SelectObject(memory, previous);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width as i32;
        info.bmiHeader.biHeight = -(height as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        assert_ne!(
            GetDIBits(
                memory,
                bitmap,
                0,
                height,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS
            ),
            0
        );
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(hwnd, dc);
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        pixels
    }
}
