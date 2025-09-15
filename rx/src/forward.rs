use std::sync::mpsc::{Receiver};
use ffmpeg_next::{codec::Id::H264, codec::decoder, packet::Packet, frame::Video as Frame};
use opencv::{core::Vector, highgui, imgproc, prelude::*};
use std::error::Error;

fn plane_to_mat(frame: &Frame, plane_index: usize, width: u32, height: u32, r_with: u32, r_height: u32) -> opencv::Result<Mat> {
    let data = frame.data(plane_index);
    let mat = Mat::new_rows_cols_with_data(height as i32, width as i32, data)?.try_clone()?;
    let mut resized_mat = Mat::default();
    imgproc::resize(&mat, &mut resized_mat, opencv::core::Size::new(r_with as i32, r_height as i32), 0.0, 0.0, imgproc::INTER_LINEAR)?;
    Ok(resized_mat)
}


pub fn forward_thread(rx: Receiver<Vec<u8>>) -> Result<(), Box<dyn Error>> {
    // Initialize FFmpeg
    ffmpeg_next::init()?;

    // Find the H.264 decoder codec
    let h264_codec = decoder::find(H264).expect("H264 decoder not found");

    // Create a video decoder context for H.264
    let mut video_decoder = decoder::new().open_as(h264_codec)?.video()?;

    // Initialize OpenCV window for displaying video frames
    highgui::named_window("Decoded Frame", highgui::WINDOW_AUTOSIZE)?;

    // Loop to receive frames from the channel
    loop {
        match rx.recv() {
            Ok(encoded_frames) => {
                // Create a packet with the received encoded frames (Vec<u8>)
                let mut packet = Packet::copy(&encoded_frames);
                video_decoder.send_packet(&packet)?;

                let mut frame = Frame::empty();

                // Receive the decoded frames and process them
                while let Ok(()) = video_decoder.receive_frame(&mut frame) {
                    // Convert the decoded frame to OpenCV Mat
                    let height = frame.height();
                    let width = frame.width();

                    let new_width = 1280;
                    let new_height = 960;

                    let mat_y = plane_to_mat(&frame, 0, width, height, new_width, new_height)?;
                    let mat_u = plane_to_mat(&frame, 1, width / 2, height / 2, new_width, new_height)?;
                    let mat_v = plane_to_mat(&frame, 2, width / 2, height / 2, new_width, new_height)?;

                    let mut mat = Vector::<Mat>::new();
                    mat.push(mat_y);
                    mat.push(mat_u);
                    mat.push(mat_v);
                    let mut merged = Mat::default();  
                    opencv::core::merge(&mat, &mut merged)?;
                    
                    let mut bgr = Mat::default();
                    imgproc::cvt_color(&merged, &mut bgr, imgproc::COLOR_YUV2BGR, 0)?;


                    // Display the resized frame using OpenCV
                    highgui::imshow("Decoded Frame", &bgr)?;

                    // Wait for 1 ms and handle events (like closing the window)
                    if highgui::wait_key(1)? == 27 { // 27 is the ESC key
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error receiving data from channel: {}", e);
                break;
            }
        }
    }

    Ok(())
}
