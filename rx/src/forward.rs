use std::error::Error;
use std::sync::mpsc::Receiver;

use ffmpeg_next::{
    codec::{decoder, Id::H264},
    frame::Video as Frame,
    packet::Packet,
    format::Pixel,
    software::scaling::{context::Context as SwsContext, flag::Flags},
};
use opencv::{core, highgui, prelude::*};

pub fn forward_thread(rx: Receiver<Vec<u8>>) -> Result<(), Box<dyn Error>> {
    ffmpeg_next::init()?;
    // ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Debug);

    // H.264 decoder
    let h264 = decoder::find(H264).expect("H264 decoder not found");
    let mut dec = decoder::new().open_as(h264)?.video()?;

    // Scaler + last seen input format
    let mut scaler: Option<SwsContext> = None;
    let dst_w = 1280;
    let dst_h = 720;

    let mut last_src_w: u32 = 0;
    let mut last_src_h: u32 = 0;
    let mut last_src_fmt: Pixel = Pixel::None;

    highgui::named_window("Decoded Frame", highgui::WINDOW_AUTOSIZE)?;

    loop {
        let encoded = match rx.recv() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Receiver closed: {e}");
                break;
            }
        };

        let encoded_frame = &encoded;

        let pkt = Packet::copy(&encoded_frame);
        dec.send_packet(&pkt)?;

        let mut in_frame = Frame::empty();
        while dec.receive_frame(&mut in_frame).is_ok() {

            let src_fmt = in_frame.format();
            let src_w = in_frame.width() ;
            let src_h = in_frame.height() ;

            if scaler.is_none() || src_w != last_src_w || src_h != last_src_h || src_fmt != last_src_fmt {
                scaler = Some(SwsContext::get(
                    src_fmt, src_w, src_h,
                    Pixel::BGR24, dst_w, dst_h, Flags::BILINEAR,
                )?);
                last_src_w = src_w ;
                last_src_h = src_h ;
                last_src_fmt = src_fmt;
            }

            // Prepare destination frame (must allocate!)
            let mut out = Frame::empty();
            out.set_format(Pixel::BGR24);
            out.set_width(dst_w);
            out.set_height(dst_h);
            unsafe {
                out.alloc(Pixel::BGR24, dst_w as u32, dst_h as u32);
            }

            // Scale/convert
            scaler.as_mut().unwrap().run(&in_frame, &mut out)?;

            // Wrap FFmpeg buffer into an OpenCV Mat by copying rows (handles stride)
            let data = out.data(0);
            let src_stride = out.stride(0) as usize;     // bytes per row in FFmpeg buffer
            let row_bytes = (dst_w as usize) * 3;        // BGR24
            let total_bytes = (dst_h as usize) * row_bytes;

            let mut mat = unsafe { core::Mat::new_rows_cols(dst_h as i32, dst_w as i32, core::CV_8UC3) }?;
            {
                let dst_ptr = mat.data_mut(); // *mut u8
                let dst = unsafe { std::slice::from_raw_parts_mut(dst_ptr, total_bytes) };
                for y in 0..(dst_h as usize) {
                    let src_off = y * src_stride;
                    let dst_off = y * row_bytes;
                    dst[dst_off..dst_off + row_bytes]
                        .copy_from_slice(&data[src_off..src_off + row_bytes]);
                }
            }

            highgui::imshow("Decoded Frame", &mat)?;

            if highgui::wait_key(1)? == 27 {
                return Ok(());
            }
        }
    }

    Ok(())
}
