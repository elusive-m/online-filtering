use parking_lot::Mutex;
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use super::Serial;

pub fn spawn_transmitter(
    serial: Serial,
    data: Arc<Vec<f32>>,
    token: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || transmitter(serial, data.as_ref(), token.as_ref()))
}

pub fn spawn_receiver(serial: Serial, capacity: usize) -> (Arc<Mutex<Vec<f32>>>, JoinHandle<()>) {
    let output = Arc::new(Mutex::new(Vec::with_capacity(capacity)));
    let handle = {
        let output = Arc::clone(&output);
        thread::spawn(move || {
            receiver(serial, output.as_ref());
        })
    };

    (output, handle)
}

fn transmitter(mut serial: Serial, samples: &[f32], token: &AtomicBool) {
    for sample in samples.iter().copied().map(f32::to_le_bytes) {
        if token.load(Ordering::Relaxed) {
            tracing::info!("Ending transmission: cancellation ordered");
            break;
        }

        if let Err(e) = serial.write_all(&sample) {
            tracing::error!("Failed to transmit `{sample:?}`: {e}");
            break;
        }
    }

    match serial.write_all(crate::EOT) {
        Ok(()) => tracing::info!("Transmission ended"),
        Err(e) => tracing::error!("Failed to complete transmission: {e}"),
    }
}

fn receiver(mut serial: Serial, output: &Mutex<Vec<f32>>) {
    let mut buffer = [0u8; std::mem::size_of::<f32>()];

    loop {
        if let Err(e) = serial.read_exact(&mut buffer) {
            tracing::error!("Failed to read sample: {e}");
            break;
        }

        if buffer == crate::EOT {
            tracing::info!("Ending reception: EOT");
            break;
        }

        output.lock().push(f32::from_le_bytes(buffer));
    }

    tracing::info!("Reception ended");
}
