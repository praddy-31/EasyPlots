//! `fft`: how strong each frequency is in a column, from evenly spaced samples.
//!
//! The steps, in order: sort by x, put the samples on an even spacing if they aren't
//! already (straight lines between neighbours), take away the mean, multiply by a window
//! (Hann unless `window=none`) so the ends don't add false frequencies, and run a fast
//! Fourier transform (padding with zeros to a power of two). The result is scaled so a
//! sine of size A shows a peak of about A (`scale=amplitude`), or as power per unit of
//! frequency (`scale=power`) or in decibels (`scale=db`). Frequencies are in cycles per
//! unit of x: x in seconds gives Hz.

use crate::spec::{FftScale, FftWindow};

/// Frequencies (from the lowest above zero up to half the sampling rate) and values.
pub fn spectrum(
    x: &[f64],
    y: &[f64],
    scale: FftScale,
    window: FftWindow,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    let mut pts: Vec<(f64, f64)> = x
        .iter()
        .zip(y)
        .map(|(a, b)| (*a, *b))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .collect();
    if pts.len() < 8 {
        return Err("needs at least 8 numbers".into());
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.dedup_by(|a, b| a.0 == b.0);
    let n = pts.len();
    let (x0, x1) = (pts[0].0, pts[n - 1].0);
    if x1 <= x0 {
        return Err("needs x values that differ".into());
    }
    let dt = (x1 - x0) / (n - 1) as f64;
    let even = pts
        .windows(2)
        .all(|w| ((w[1].0 - w[0].0) - dt).abs() <= dt * 0.01);
    let mut values: Vec<f64> = if even {
        pts.iter().map(|p| p.1).collect()
    } else {
        // Uneven spacing: read the curve off at even steps.
        let mut j = 0;
        (0..n)
            .map(|i| {
                let t = x0 + dt * i as f64;
                while j + 2 < n && pts[j + 1].0 < t {
                    j += 1;
                }
                let (a, b) = (pts[j], pts[j + 1]);
                let f = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
                a.1 + (b.1 - a.1) * f
            })
            .collect()
    };
    let mean = values.iter().sum::<f64>() / n as f64;
    let w: Vec<f64> = (0..n)
        .map(|i| match window {
            FftWindow::Hann => {
                0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos()
            }
            FftWindow::None => 1.0,
        })
        .collect();
    for (v, wi) in values.iter_mut().zip(&w) {
        *v = (*v - mean) * wi;
    }
    let size = n.next_power_of_two();
    let mut re = values;
    re.resize(size, 0.0);
    let mut im = vec![0.0; size];
    fft(&mut re, &mut im);

    let sum_w: f64 = w.iter().sum();
    let sum_w2: f64 = w.iter().map(|v| v * v).sum();
    let rate = 1.0 / dt;
    let (mut fs, mut out) = (Vec::new(), Vec::new());
    for k in 1..=size / 2 {
        let mag2 = re[k] * re[k] + im[k] * im[k];
        // One-sided: the negative frequencies' half is folded in (×2), except at the top.
        let twice = if k == size / 2 { 1.0 } else { 2.0 };
        let amplitude = twice * mag2.sqrt() / sum_w;
        fs.push(k as f64 * rate / size as f64);
        out.push(match scale {
            FftScale::Amplitude => amplitude,
            FftScale::Power => twice * mag2 / (rate * sum_w2),
            FftScale::Db => 20.0 * amplitude.max(1e-300).log10(),
        });
    }
    Ok((fs, out))
}

/// In-place radix-2 fast Fourier transform; the length must be a power of two.
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, dt: f64, f: f64, a: f64) -> (Vec<f64>, Vec<f64>) {
        let x: Vec<f64> = (0..n).map(|i| i as f64 * dt).collect();
        let y = x
            .iter()
            .map(|t| 3.0 + a * (2.0 * std::f64::consts::PI * f * t).sin())
            .collect();
        (x, y)
    }

    fn peak(fs: &[f64], v: &[f64]) -> (f64, f64) {
        let i = (0..v.len()).max_by(|a, b| v[*a].total_cmp(&v[*b])).unwrap();
        (fs[i], v[i])
    }

    #[test]
    fn a_sine_peaks_at_its_frequency_with_its_size() {
        // 1024 samples at 100 Hz, a 12.5 Hz sine of size 2 (exactly on a frequency step).
        let (x, y) = sine(1024, 0.01, 12.5, 2.0);
        let (fs, a) = spectrum(&x, &y, FftScale::Amplitude, FftWindow::Hann).unwrap();
        let (f, v) = peak(&fs, &a);
        assert!((f - 12.5).abs() < 0.1, "{f}");
        assert!((v - 2.0).abs() < 0.05, "{v}");
        assert!((fs.last().unwrap() - 50.0).abs() < 1e-9);
        // Without a window too.
        let (_, a) = spectrum(&x, &y, FftScale::Amplitude, FftWindow::None).unwrap();
        assert!((peak(&fs, &a).1 - 2.0).abs() < 0.05);
        // Decibels: 20·log10(2).
        let (_, d) = spectrum(&x, &y, FftScale::Db, FftWindow::Hann).unwrap();
        assert!((peak(&fs, &d).1 - 6.02).abs() < 0.3);
    }

    #[test]
    fn uneven_or_unsorted_samples_still_work() {
        let (mut x, mut y) = sine(1000, 0.01, 10.0, 1.0);
        // Shuffle a little and jitter one sample.
        x.swap(3, 400);
        y.swap(3, 400);
        x[10] += 0.003;
        let (fs, a) = spectrum(&x, &y, FftScale::Amplitude, FftWindow::Hann).unwrap();
        let (f, v) = peak(&fs, &a);
        assert!((f - 10.0).abs() < 0.2, "{f}");
        assert!((v - 1.0).abs() < 0.15, "{v}");
        assert!(spectrum(&x[..5], &y[..5], FftScale::Amplitude, FftWindow::Hann).is_err());
    }
}
