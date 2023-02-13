use crate::audio::format::adpcm::{Coefficients, Info, BYTES_PER_FRAME, SAMPLES_PER_FRAME};
use crate::common::clamp_i16;

pub(crate) fn encode(pcm: &[i16], info: &mut Info) -> Vec<u8> {
    let num_frames = (pcm.len() + SAMPLES_PER_FRAME - 1) / SAMPLES_PER_FRAME;
    let mut adpcm = Vec::with_capacity(num_frames * BYTES_PER_FRAME);

    let mut adpcm_buf = [0; BYTES_PER_FRAME];
    let mut pcm_buf = [0; SAMPLES_PER_FRAME + 2];
    pcm_buf[0] = info.context.last_samples[1];
    pcm_buf[1] = info.context.last_samples[0];

    // Encode frame-by-frame
    for samples in pcm.chunks(SAMPLES_PER_FRAME) {
        // The first two pcm_buf values are from the last frame, the rest are from this frame
        let end = samples.len() + 2;
        pcm_buf[2..end].copy_from_slice(samples);
        pcm_buf[end..].fill(0);

        // Write ADPCM bytes to adpcm_buf and update pcm_buf with the re-decoded samples
        encode_frame(&mut pcm_buf, &info.coefficients, &mut adpcm_buf);

        // Copy the last two samples back to the beginning
        pcm_buf.copy_within((end - 2)..end, 0);

        // Append the encoded frame and discard unused bytes
        let frame_size = 1 + (samples.len() + 1) / 2;
        adpcm.extend(&adpcm_buf[..frame_size]);
    }

    info.context.last_samples[1] = pcm_buf[0];
    info.context.last_samples[0] = pcm_buf[1];
    adpcm
}

fn encode_frame(pcm: &mut [i16], coefs: &Coefficients, out: &mut [u8]) {
    // Encode using all possible coefficient pairs
    let mut frames = [Frame::default(); 8];
    for (c, frame) in coefs.chunks(2).zip(&mut frames) {
        *frame = try_coefficients(pcm, c[0].into(), c[1].into());
    }

    // Take the closest one
    let mut best_index = 0;
    for (i, encoding) in frames.iter().enumerate().skip(1) {
        if encoding.distance < frames[best_index].distance {
            best_index = i;
        }
    }
    let best = &frames[best_index];
    pcm[2..].copy_from_slice(&best.pcm[2..]);

    // Frames start with the predictor/scale byte
    let predictor = (best_index as u8) & 0xf;
    let scale = (best.power as u8) & 0xf;
    out[0] = (predictor << 4) | scale;

    // Pack samples into nibbles
    for (adpcm, o) in best.adpcm.chunks(2).zip(&mut out[1..]) {
        *o = (((adpcm[0] as u8) & 0xf) << 4) | ((adpcm[1] as u8) & 0xf);
    }
}

#[derive(Copy, Clone, Default)]
struct Frame {
    pcm: [i16; SAMPLES_PER_FRAME + 2],
    adpcm: [i32; SAMPLES_PER_FRAME],
    power: i32,
    distance: f64,
}

fn try_coefficients(pcm: &[i16], c0: i32, c1: i32) -> Frame {
    assert!(pcm.len() >= 2 && pcm.len() <= SAMPLES_PER_FRAME + 2);

    let mut frame = Frame::default();
    frame.pcm[0] = pcm[0];
    frame.pcm[1] = pcm[1];

    let mut max_distance: i16 = 0;
    for s in pcm.windows(3) {
        let predicted = (c0 * (s[1] as i32) + c1 * (s[0] as i32)) / 2048;
        let distance = clamp_i16(s[2] as i32 - predicted);
        if distance == i16::MIN {
            max_distance = i16::MIN;
            break;
        } else if distance.abs() > max_distance.abs() {
            max_distance = distance;
        }
    }

    let mut power = 0;
    while power <= 12 && !(-8..=7).contains(&max_distance) {
        max_distance /= 2;
        power += 1;
    }
    power = (power - 2).max(-1);

    loop {
        power += 1;
        let scale = (1 << power) * 2048;
        frame.distance = 0.0;
        let mut max_overflow = 0;

        for (s, adpcm) in frame.adpcm.iter_mut().enumerate() {
            let s0 = frame.pcm[s + 1] as i32;
            let s1 = frame.pcm[s] as i32;
            let predicted = s0 * c0 + s1 * c1;
            let distance = ((pcm[s + 2] as i32) * 2048) - predicted;

            let unclamped = if distance > 0 {
                ((distance as f32 / scale as f32) as f64 + 0.4999999) as i32
            } else {
                ((distance as f32 / scale as f32) as f64 - 0.4999999) as i32
            };

            let clamped = unclamped.clamp(-8, 7);
            if clamped != unclamped {
                let overflow = (unclamped - clamped).abs();
                max_overflow = max_overflow.max(overflow);
            }
            *adpcm = clamped;
            frame.pcm[s + 2] = clamp_i16((predicted + clamped * scale + 0x400) >> 11);

            let actual_distance = (pcm[s + 2] as i32 - frame.pcm[s + 2] as i32) as f64;
            frame.distance += actual_distance * actual_distance;
        }

        let mut x = max_overflow + 8;
        while x > 256 {
            power = (power + 1).min(11);
            x >>= 1;
        }
        if power >= 12 || max_overflow <= 1 {
            break;
        }
    }

    frame.power = power;
    frame
}
