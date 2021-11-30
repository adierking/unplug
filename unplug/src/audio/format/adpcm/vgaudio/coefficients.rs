#![allow(clippy::needless_range_loop)]

use crate::audio::format::adpcm::{Coefficients, SAMPLES_PER_FRAME};

pub(crate) type Matrix3 = [[f64; 3]; 3];
pub(crate) type Vec3 = [f64; 3];

pub(crate) type PcmHistory = [i16; SAMPLES_PER_FRAME * 2];

pub(crate) fn process_frame(pcm_hist: &mut PcmHistory, records: &mut Vec<Vec3>) {
    let mut vec1 = Vec3::default();
    let mut buffer = Vec3::default();
    let mut mtx = Matrix3::default();
    let mut vec_idxs = [0; 3];
    inner_product_merge(&mut vec1, pcm_hist);
    if vec1[0].abs() > 10.0 {
        outer_product_merge(&mut mtx, pcm_hist);
        if !analyze_ranges(&mut mtx, &mut vec_idxs, &mut buffer) {
            bidirectional_filter(&mtx, &vec_idxs, &mut vec1);
            if !quadratic_merge(&mut vec1) {
                let mut record = Vec3::default();
                finish_record(&vec1, &mut record);
                records.push(record);
            }
        }
    }
    pcm_hist.copy_within(SAMPLES_PER_FRAME.., 0);
}

pub(crate) fn finish(records: &[Vec3]) -> Coefficients {
    let mut mtx = Matrix3::default();
    let mut vec1 = [1.0, 0.0, 0.0];
    let mut vec_best = [Vec3::default(); 8];
    for record in records {
        matrix_filter(record, &mut vec_best[0], &mut mtx);
        for y in 1..=2 {
            vec1[y] += vec_best[0][y];
        }
    }
    for y in 1..=2 {
        vec1[y] /= records.len() as f64;
    }

    merge_finish_record(&vec1, &mut vec_best[0]);

    let mut exp = 1;
    for w in 0..3 {
        let vec2 = [0.0, -1.0, 0.0];
        for i in 0..exp {
            for y in 0..=2 {
                vec_best[exp + i][y] = (0.01 * vec2[y]) + vec_best[i][y];
            }
        }
        exp = 1 << (w + 1);
        filter_records(&mut vec_best, exp, records);
    }

    let mut coefs = Coefficients::default();
    for z in 0..8 {
        let d = -vec_best[z][1] * 2048.0;
        coefs[z * 2] = if d > 0.0 {
            d.min(i16::MAX as f64).round() as i16
        } else {
            d.max(i16::MIN as f64).round() as i16
        };

        let d = -vec_best[z][2] * 2048.0;
        coefs[z * 2 + 1] = if d > 0.0 {
            d.min(i16::MAX as f64).round() as i16
        } else {
            d.max(i16::MIN as f64).round() as i16
        };
    }
    coefs
}

fn inner_product_merge(vec_out: &mut Vec3, pcm: &PcmHistory) {
    for i in 0..=2 {
        vec_out[i] = 0.0;
        for x in 0..SAMPLES_PER_FRAME {
            vec_out[i] -= ((pcm[SAMPLES_PER_FRAME + x - i] as i32)
                * (pcm[SAMPLES_PER_FRAME + x] as i32)) as f64;
        }
    }
}

fn outer_product_merge(mtx_out: &mut Matrix3, pcm: &PcmHistory) {
    for x in 1..=2 {
        for y in 1..=2 {
            mtx_out[x][y] = 0.0;
            for z in 0..SAMPLES_PER_FRAME {
                mtx_out[x][y] += ((pcm[SAMPLES_PER_FRAME + z - x] as i32)
                    * (pcm[SAMPLES_PER_FRAME + z - y] as i32))
                    as f64;
            }
        }
    }
}

fn analyze_ranges(mtx: &mut Matrix3, vec_idxs_out: &mut [usize; 3], recips: &mut Vec3) -> bool {
    for x in 1..=2 {
        let max = mtx[x][1].abs().max(mtx[x][2].abs());
        if max < f64::EPSILON {
            return true;
        }
        recips[x] = 1.0 / max;
    }

    let mut max_index = 0;
    for i in 1..=2 {
        for x in 1..i {
            let mut tmp = mtx[x][i];
            for y in 1..x {
                tmp -= mtx[x][y] * mtx[y][i];
            }
            mtx[x][i] = tmp;
        }

        let mut max = 0.0;
        for x in i..=2 {
            let mut tmp = mtx[x][i];
            for y in 1..i {
                tmp -= mtx[x][y] * mtx[y][i];
            }

            mtx[x][i] = tmp;
            tmp = tmp.abs() * recips[x];
            if tmp >= max {
                max = tmp;
                max_index = x;
            }
        }

        if max_index != i {
            for y in 1..=2 {
                let tmp = mtx[max_index][y];
                mtx[max_index][y] = mtx[i][y];
                mtx[i][y] = tmp;
            }
            recips[max_index] = recips[i];
        }

        vec_idxs_out[i] = max_index;

        if i == 1 {
            mtx[2][1] *= 1.0 / mtx[1][1];
        }
    }

    let mut min: f64 = 1.0e10;
    let mut max: f64 = 0.0;
    for i in 1..=2 {
        let tmp = mtx[i][i].abs();
        min = min.min(tmp);
        max = max.max(tmp);
    }
    min / max < 1.0e-10
}

fn bidirectional_filter(mtx: &Matrix3, vec_idxs: &[usize; 3], vec_out: &mut Vec3) {
    let mut x = 0;
    for i in 1..=2 {
        let index = vec_idxs[i];
        let mut tmp = vec_out[index];
        vec_out[index] = vec_out[i];
        if x != 0 {
            for y in x..i {
                tmp -= vec_out[y] * mtx[i][y];
            }
        } else if tmp != 0.0 {
            x = i;
        }
        vec_out[i] = tmp;
    }

    for i in (1..=2).rev() {
        let mut tmp = vec_out[i];
        for y in (i + 1)..=2 {
            tmp -= vec_out[y] * mtx[i][y];
        }
        vec_out[i] = tmp / mtx[i][i];
    }

    vec_out[0] = 1.0;
}

fn quadratic_merge(vec: &mut Vec3) -> bool {
    let v2 = vec[2];
    let tmp = 1.0 - (v2 * v2);
    if tmp == 0.0 {
        return true;
    }

    let v0 = (vec[0] - (v2 * v2)) / tmp;
    let v1 = (vec[1] - (vec[1] * v2)) / tmp;
    *vec = [v0, v1, v2];
    v1.abs() > 1.0
}

fn finish_record(in_record: &Vec3, out_record: &mut Vec3) {
    let mut in_record = *in_record;
    for z in 1..=2 {
        if in_record[z] >= 1.0 {
            in_record[z] = 0.9999999999;
        } else if in_record[z] <= -1.0 {
            in_record[z] = -0.9999999999;
        }
    }
    *out_record = [1.0, (in_record[2] * in_record[1]) + in_record[1], in_record[2]];
}

fn matrix_filter(src: &Vec3, dst: &mut Vec3, mtx: &mut Matrix3) {
    mtx[2][0] = 1.0;
    for i in 1..=2 {
        mtx[2][i] = -src[i];
    }

    for i in (1..=2).rev() {
        let val = 1.0 - (mtx[i][i] * mtx[i][i]);
        for y in 1..=i {
            mtx[i - 1][y] = ((mtx[i][i] * mtx[i][y]) + mtx[i][y]) / val;
        }
    }

    dst[0] = 1.0;
    for i in 1..=2 {
        dst[i] = 0.0;
        for y in 1..=i {
            dst[i] += mtx[i][y] * dst[i - y];
        }
    }
}

fn merge_finish_record(src: &Vec3, dst: &mut Vec3) {
    let mut tmp = Vec3::default();
    let mut val = src[0];

    dst[0] = 1.0;
    for i in 1..=2 {
        let mut v2 = 0.0;
        for y in 1..i {
            v2 += dst[y] * src[i - y];
        }

        dst[i] = if val > 0.0 { -(v2 + src[i]) / val } else { 0.0 };
        tmp[i] = dst[i];

        for y in 1..i {
            dst[y] += dst[i] * dst[i - y];
        }

        val *= 1.0 - (dst[i] * dst[i]);
    }

    finish_record(&tmp, dst);
}

fn contrast_vectors(a: &Vec3, b: &Vec3) -> f64 {
    let val = (b[2] * b[1] + -b[1]) / (1.0 - b[2] * b[2]);
    let val1 = (a[0] * a[0]) + (a[1] * a[1]) + (a[2] * a[2]);
    let val2 = (a[0] * a[1]) + (a[1] * a[2]);
    let val3 = a[0] * a[2];
    val1 + (2.0 * val * val2) + (2.0 * (-b[1] * val + -b[2]) * val3)
}

fn filter_records(vec_best: &mut [Vec3], exp: usize, records: &[Vec3]) {
    let mut buffer1 = [0i32; 8];
    let mut buffer2 = [0.0f64; 3];
    let mut buffer_list = [Vec3::default(); 8];
    let mut mtx = [Vec3::default(); 3];

    for _x in 0..2 {
        for y in 0..exp {
            buffer1[y] = 0;
            for i in 0..=2 {
                buffer_list[y][i] = 0.0;
            }
        }
        for record in records {
            let mut index = 0;
            let mut value = 1.0e30;
            for i in 0..exp {
                let temp = contrast_vectors(&vec_best[i], record);
                if temp < value {
                    value = temp;
                    index = i;
                }
            }
            buffer1[index] += 1;
            matrix_filter(record, &mut buffer2, &mut mtx);
            for i in 0..=2 {
                buffer_list[index][i] += buffer2[i];
            }
        }

        for i in 0..exp {
            if buffer1[i] > 0 {
                for y in 0..=2 {
                    buffer_list[i][y] /= buffer1[i] as f64;
                }
            }
        }

        for i in 0..exp {
            merge_finish_record(&buffer_list[i], &mut vec_best[i]);
        }
    }
}
