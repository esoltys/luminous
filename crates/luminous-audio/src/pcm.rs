//! Pure, allocation-free PCM format conversions and channel de-interleaving.
//!
//! Converts interleaved `&[f32]` samples from `OutputRenderer` into the
//! native sample formats expected by hardware audio drivers (such as ASIO):
//! - 16-bit signed integer PCM (`i16`)
//! - 24-bit signed integer PCM in 3-byte packed storage
//! - 32-bit signed integer PCM (`i32`)
//! - 32-bit signed integer with 24-bit precision
//! - 32-bit floating point (`f32`)

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved `i16` buffers.
///
/// # Panics
/// Panics if `left.len()` or `right.len()` is less than `interleaved.len() / 2`.
#[inline]
pub fn interleave_to_deinterleaved_i16(
    interleaved: &[f32],
    left: &mut [i16],
    right: &mut [i16],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        let l = interleaved[i * 2];
        let r = interleaved[i * 2 + 1];
        left[i] = f32_to_i16(l);
        right[i] = f32_to_i16(r);
    }
    n
}

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved `i32` buffers.
#[inline]
pub fn interleave_to_deinterleaved_i32(
    interleaved: &[f32],
    left: &mut [i32],
    right: &mut [i32],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        let l = interleaved[i * 2];
        let r = interleaved[i * 2 + 1];
        left[i] = f32_to_i32(l);
        right[i] = f32_to_i32(r);
    }
    n
}

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved 24-bit in 32-bit container buffers.
#[inline]
pub fn interleave_to_deinterleaved_i32_24(
    interleaved: &[f32],
    left: &mut [i32],
    right: &mut [i32],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        let l = interleaved[i * 2];
        let r = interleaved[i * 2 + 1];
        left[i] = f32_to_i24_in_i32(l);
        right[i] = f32_to_i24_in_i32(r);
    }
    n
}

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved packed 24-bit buffers (3 bytes per sample).
#[inline]
pub fn interleave_to_deinterleaved_i24_packed(
    interleaved: &[f32],
    left: &mut [u8],
    right: &mut [u8],
) -> usize {
    let frames = interleaved.len() / 2;
    let max_frames = (left.len() / 3).min(right.len() / 3);
    let n = frames.min(max_frames);
    for i in 0..n {
        let l = f32_to_i24_packed(interleaved[i * 2]);
        let r = f32_to_i24_packed(interleaved[i * 2 + 1]);
        left[i * 3] = l[0];
        left[i * 3 + 1] = l[1];
        left[i * 3 + 2] = l[2];
        right[i * 3] = r[0];
        right[i * 3 + 1] = r[1];
        right[i * 3 + 2] = r[2];
    }
    n
}

/// Copies and de-interleaves stereo `f32` samples into two `f32` buffers.
#[inline]
pub fn interleave_to_deinterleaved_f32(
    interleaved: &[f32],
    left: &mut [f32],
    right: &mut [f32],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        left[i] = interleaved[i * 2];
        right[i] = interleaved[i * 2 + 1];
    }
    n
}

// The float → integer converters scale by 2^(bits-1), the exact inverse of
// Symphonia's integer → float decode (`s / 32768.0` for 16-bit), so an
// untouched sample reaches the device with the bits it had in the file. The
// `as` casts saturate, which pins +1.0 to the type's maximum.

#[inline]
pub fn f32_to_i16(sample: f32) -> i16 {
    (sample * 32_768.0).round() as i16
}

#[inline]
pub fn f32_to_i32(sample: f32) -> i32 {
    (sample as f64 * 2_147_483_648.0).round() as i32
}

/// A 24-bit sample, right-justified (sign-extended) in an `i32`.
#[inline]
pub fn f32_to_i24_in_i32(sample: f32) -> i32 {
    ((sample * 8_388_608.0).round() as i32).clamp(-8_388_608, 8_388_607)
}

#[inline]
pub fn f32_to_i24_packed(sample: f32) -> [u8; 3] {
    let val = f32_to_i24_in_i32(sample);
    let bytes = val.to_le_bytes();
    [bytes[0], bytes[1], bytes[2]]
}

#[inline]
pub fn f32_to_i24_packed_be(sample: f32) -> [u8; 3] {
    let val = f32_to_i24_in_i32(sample);
    let bytes = val.to_be_bytes();
    [bytes[1], bytes[2], bytes[3]]
}

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved `i16` big-endian buffers.
#[inline]
pub fn interleave_to_deinterleaved_i16_be(
    interleaved: &[f32],
    left: &mut [i16],
    right: &mut [i16],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        left[i] = f32_to_i16(interleaved[i * 2]).to_be();
        right[i] = f32_to_i16(interleaved[i * 2 + 1]).to_be();
    }
    n
}

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved `i32` big-endian buffers.
#[inline]
pub fn interleave_to_deinterleaved_i32_be(
    interleaved: &[f32],
    left: &mut [i32],
    right: &mut [i32],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        left[i] = f32_to_i32(interleaved[i * 2]).to_be();
        right[i] = f32_to_i32(interleaved[i * 2 + 1]).to_be();
    }
    n
}

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved 24-in-32 big-endian buffers.
#[inline]
pub fn interleave_to_deinterleaved_i32_24_be(
    interleaved: &[f32],
    left: &mut [i32],
    right: &mut [i32],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        left[i] = f32_to_i24_in_i32(interleaved[i * 2]).to_be();
        right[i] = f32_to_i24_in_i32(interleaved[i * 2 + 1]).to_be();
    }
    n
}

/// Copies and converts interleaved stereo `f32` samples into two de-interleaved packed 24-bit big-endian buffers.
#[inline]
pub fn interleave_to_deinterleaved_i24_packed_be(
    interleaved: &[f32],
    left: &mut [u8],
    right: &mut [u8],
) -> usize {
    let frames = interleaved.len() / 2;
    let max_frames = (left.len() / 3).min(right.len() / 3);
    let n = frames.min(max_frames);
    for i in 0..n {
        let l = f32_to_i24_packed_be(interleaved[i * 2]);
        let r = f32_to_i24_packed_be(interleaved[i * 2 + 1]);
        left[i * 3] = l[0];
        left[i * 3 + 1] = l[1];
        left[i * 3 + 2] = l[2];
        right[i * 3] = r[0];
        right[i * 3 + 1] = r[1];
        right[i * 3 + 2] = r[2];
    }
    n
}

/// Copies and de-interleaves stereo `f32` samples into two big-endian `u32` buffers.
#[inline]
pub fn interleave_to_deinterleaved_f32_be(
    interleaved: &[f32],
    left: &mut [u32],
    right: &mut [u32],
) -> usize {
    let frames = interleaved.len() / 2;
    let n = frames.min(left.len()).min(right.len());
    for i in 0..n {
        left[i] = interleaved[i * 2].to_bits().to_be();
        right[i] = interleaved[i * 2 + 1].to_bits().to_be();
    }
    n
}

/// An interleaved little-endian integer PCM layout a device accepts, as
/// negotiated with WASAPI exclusive mode or ALSA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmFormat {
    /// 32-bit container, 32 valid bits.
    I32,
    /// 32-bit container, 24 valid bits left-justified (the low byte is zero).
    I24In32,
    /// 32-bit container, 24 valid bits right-justified (sign-extended into
    /// the top byte): ALSA's `S24_LE`.
    I24In32Low,
    /// 3-byte packed 24-bit samples.
    I24Packed,
    I16,
}

impl PcmFormat {
    /// WASAPI's candidates, deepest first: every one holds a 16- or 24-bit
    /// source losslessly, and a wider one leaves the gain chain more resolution.
    pub const PREFERENCE: [PcmFormat; 4] = [Self::I32, Self::I24In32, Self::I24Packed, Self::I16];

    /// ALSA's candidates in the same order (`S32_LE`, `S24_LE`, `S24_3LE`,
    /// `S16_LE`): ALSA's 24-in-32 layout is right-justified.
    pub const ALSA_PREFERENCE: [PcmFormat; 4] =
        [Self::I32, Self::I24In32Low, Self::I24Packed, Self::I16];

    /// Bytes one sample occupies in the device buffer.
    pub fn container_bytes(self) -> usize {
        match self {
            Self::I32 | Self::I24In32 | Self::I24In32Low => 4,
            Self::I24Packed => 3,
            Self::I16 => 2,
        }
    }

    /// Significant bits per sample.
    pub fn valid_bits(self) -> u16 {
        match self {
            Self::I32 => 32,
            Self::I24In32 | Self::I24In32Low | Self::I24Packed => 24,
            Self::I16 => 16,
        }
    }

    /// The pipeline display's name for this format, e.g. "24-bit integer PCM".
    pub fn label(self) -> &'static str {
        match self {
            Self::I32 => "32-bit integer PCM",
            Self::I24In32 | Self::I24In32Low | Self::I24Packed => "24-bit integer PCM",
            Self::I16 => "16-bit integer PCM",
        }
    }
}

/// Converts interleaved `f32` samples into `dst` in `format`, sample for
/// sample (channel order is kept). Writes as many whole samples as both sides
/// hold and returns that count. Allocation-free: safe on the audio thread.
pub fn write_interleaved(src: &[f32], format: PcmFormat, dst: &mut [u8]) -> usize {
    let width = format.container_bytes();
    let n = src.len().min(dst.len() / width);
    let out = dst[..n * width].chunks_exact_mut(width);
    match format {
        PcmFormat::I32 => {
            for (s, o) in src.iter().zip(out) {
                o.copy_from_slice(&f32_to_i32(*s).to_le_bytes());
            }
        }
        PcmFormat::I24In32 => {
            for (s, o) in src.iter().zip(out) {
                o.copy_from_slice(&(f32_to_i24_in_i32(*s) << 8).to_le_bytes());
            }
        }
        PcmFormat::I24In32Low => {
            for (s, o) in src.iter().zip(out) {
                o.copy_from_slice(&f32_to_i24_in_i32(*s).to_le_bytes());
            }
        }
        PcmFormat::I24Packed => {
            for (s, o) in src.iter().zip(out) {
                o.copy_from_slice(&f32_to_i24_packed(*s));
            }
        }
        PcmFormat::I16 => {
            for (s, o) in src.iter().zip(out) {
                o.copy_from_slice(&f32_to_i16(*s).to_le_bytes());
            }
        }
    }
    n
}

/// Native sample formats supported by ASIO drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsioSampleFormat {
    Int16LSB,
    Int24LSB,
    Int32LSB,
    Int32LSB24,
    Float32LSB,
    Int16MSB,
    Int24MSB,
    Int32MSB,
    Int32MSB24,
    Float32MSB,
    Unknown(i32),
}

impl AsioSampleFormat {
    /// Returns true if this sample format is supported by the format converter.
    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }

    /// Size in bytes of a single sample in this format.
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            Self::Int16LSB | Self::Int16MSB => 2,
            Self::Int24LSB | Self::Int24MSB => 3,
            Self::Int32LSB
            | Self::Int32LSB24
            | Self::Int32MSB
            | Self::Int32MSB24
            | Self::Float32LSB
            | Self::Float32MSB => 4,
            Self::Unknown(_) => 0,
        }
    }

    /// The pipeline display's name for this format, e.g. "24-bit integer PCM".
    pub fn label(&self) -> &'static str {
        match self {
            Self::Int16LSB | Self::Int16MSB => "16-bit integer PCM",
            Self::Int24LSB | Self::Int24MSB | Self::Int32LSB24 | Self::Int32MSB24 => {
                "24-bit integer PCM"
            }
            Self::Int32LSB | Self::Int32MSB => "32-bit integer PCM",
            Self::Float32LSB | Self::Float32MSB => "32-bit float PCM",
            Self::Unknown(_) => "Unknown PCM",
        }
    }
}

#[cfg(windows)]
impl From<azo::sys::SampleType> for AsioSampleFormat {
    fn from(st: azo::sys::SampleType) -> Self {
        match st {
            azo::sys::SampleType::PCM_I16_LSB => Self::Int16LSB,
            azo::sys::SampleType::PCM_I24_LSB => Self::Int24LSB,
            azo::sys::SampleType::PCM_I32_LSB => Self::Int32LSB,
            azo::sys::SampleType::PCM_I32_LSB_24 => Self::Int32LSB24,
            azo::sys::SampleType::PCM_F32_LSB => Self::Float32LSB,
            azo::sys::SampleType::PCM_I16_MSB => Self::Int16MSB,
            azo::sys::SampleType::PCM_I24_MSB => Self::Int24MSB,
            azo::sys::SampleType::PCM_I32_MSB => Self::Int32MSB,
            azo::sys::SampleType::PCM_I32_MSB_24 => Self::Int32MSB24,
            azo::sys::SampleType::PCM_F32_MSB => Self::Float32MSB,
            other => Self::Unknown(other.0),
        }
    }
}

/// De-interleaves and converts stereo `f32` samples into raw channel output pointers.
///
/// # Safety
/// `left_ptr` and `right_ptr` must point to valid, writable buffers large enough to hold
/// `frames` samples of format `fmt`.
pub unsafe fn convert_and_deinterleave(
    interleaved: &[f32],
    fmt: AsioSampleFormat,
    left_ptr: *mut u8,
    right_ptr: *mut u8,
    frames: usize,
) {
    match fmt {
        AsioSampleFormat::Int16LSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i16>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i16>(), frames);
            interleave_to_deinterleaved_i16(interleaved, left, right);
        }
        AsioSampleFormat::Int16MSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i16>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i16>(), frames);
            interleave_to_deinterleaved_i16_be(interleaved, left, right);
        }
        AsioSampleFormat::Int32LSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i32>(), frames);
            interleave_to_deinterleaved_i32(interleaved, left, right);
        }
        AsioSampleFormat::Int32MSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i32>(), frames);
            interleave_to_deinterleaved_i32_be(interleaved, left, right);
        }
        AsioSampleFormat::Int32LSB24 => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i32>(), frames);
            interleave_to_deinterleaved_i32_24(interleaved, left, right);
        }
        AsioSampleFormat::Int32MSB24 => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i32>(), frames);
            interleave_to_deinterleaved_i32_24_be(interleaved, left, right);
        }
        AsioSampleFormat::Int24LSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr, frames * 3);
            let right = std::slice::from_raw_parts_mut(right_ptr, frames * 3);
            interleave_to_deinterleaved_i24_packed(interleaved, left, right);
        }
        AsioSampleFormat::Int24MSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr, frames * 3);
            let right = std::slice::from_raw_parts_mut(right_ptr, frames * 3);
            interleave_to_deinterleaved_i24_packed_be(interleaved, left, right);
        }
        AsioSampleFormat::Float32LSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<f32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<f32>(), frames);
            interleave_to_deinterleaved_f32(interleaved, left, right);
        }
        AsioSampleFormat::Float32MSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<u32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<u32>(), frames);
            interleave_to_deinterleaved_f32_be(interleaved, left, right);
        }
        AsioSampleFormat::Unknown(_) => {
            // Memory-safe zeroing: do not assume 4-byte width to prevent buffer overrun
            let byte_count = frames * fmt.bytes_per_sample();
            if byte_count > 0 {
                std::ptr::write_bytes(left_ptr, 0, byte_count);
                std::ptr::write_bytes(right_ptr, 0, byte_count);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f32_to_i16_clamping_and_extremes() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(1.5), i16::MAX);
        assert_eq!(f32_to_i16(-1.0), i16::MIN);
        assert_eq!(f32_to_i16(-2.0), i16::MIN);
        assert_eq!(f32_to_i16(0.5), 16384);
        assert_eq!(f32_to_i16(-0.5), -16384);
    }

    #[test]
    fn test_f32_to_i32_clamping_and_extremes() {
        assert_eq!(f32_to_i32(0.0), 0);
        assert_eq!(f32_to_i32(1.0), i32::MAX);
        assert_eq!(f32_to_i32(2.0), i32::MAX);
        assert_eq!(f32_to_i32(-1.0), i32::MIN);
        assert_eq!(f32_to_i32(-2.0), i32::MIN);
    }

    #[test]
    fn test_f32_to_i24_in_i32() {
        assert_eq!(f32_to_i24_in_i32(0.0), 0);
        assert_eq!(f32_to_i24_in_i32(1.0), 8388607);
        assert_eq!(f32_to_i24_in_i32(-1.0), -8388608);
    }

    #[test]
    fn integer_samples_survive_the_decode_round_trip_bit_for_bit() {
        // Symphonia decodes integers to float by dividing by 2^(bits-1).
        for s in i16::MIN..=i16::MAX {
            assert_eq!(f32_to_i16(s as f32 / 32_768.0), s);
        }
        for s in [-8_388_608, -4_194_305, -1, 0, 1, 4_194_305, 8_388_607] {
            assert_eq!(f32_to_i24_in_i32(s as f32 / 8_388_608.0), s);
        }
        for s in [i32::MIN, -1 << 30, -256, 0, 256, 1 << 30, i32::MAX & !0xFF] {
            assert_eq!(f32_to_i32((s as f64 / 2_147_483_648.0) as f32), s);
        }
    }

    #[test]
    fn write_interleaved_lays_out_each_format_little_endian() {
        let src = [0.5f32, -1.0];

        let mut i16_out = [0u8; 4];
        assert_eq!(write_interleaved(&src, PcmFormat::I16, &mut i16_out), 2);
        assert_eq!(i16_out, [0x00, 0x40, 0x00, 0x80]);

        let mut packed = [0u8; 6];
        assert_eq!(
            write_interleaved(&src, PcmFormat::I24Packed, &mut packed),
            2
        );
        assert_eq!(packed, [0x00, 0x00, 0x40, 0x00, 0x00, 0x80]);

        // 24 valid bits sit in the top three bytes of the 32-bit container.
        let mut in32 = [0u8; 8];
        assert_eq!(write_interleaved(&src, PcmFormat::I24In32, &mut in32), 2);
        assert_eq!(in32, [0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x80]);

        // ALSA's S24_LE: 24 valid bits in the low three bytes, sign-extended.
        let mut in32_low = [0u8; 8];
        assert_eq!(
            write_interleaved(&src, PcmFormat::I24In32Low, &mut in32_low),
            2
        );
        assert_eq!(in32_low, [0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x80, 0xFF]);

        let mut i32_out = [0u8; 8];
        assert_eq!(write_interleaved(&src, PcmFormat::I32, &mut i32_out), 2);
        assert_eq!(i32_out, [0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x80]);
    }

    #[test]
    fn write_interleaved_stops_at_the_shorter_side() {
        let mut dst = [0xAAu8; 5];
        assert_eq!(write_interleaved(&[0.0; 4], PcmFormat::I16, &mut dst), 2);
        assert_eq!(dst, [0, 0, 0, 0, 0xAA]);
    }

    #[test]
    fn test_deinterleave_i16() {
        let interleaved = [0.0, 1.0, -1.0, 0.5];
        let mut left = [0i16; 2];
        let mut right = [0i16; 2];
        let count = interleave_to_deinterleaved_i16(&interleaved, &mut left, &mut right);
        assert_eq!(count, 2);
        assert_eq!(left, [0, i16::MIN]);
        assert_eq!(right, [i16::MAX, 16384]);
    }

    #[test]
    fn test_deinterleave_i32() {
        let interleaved = [0.0, 1.0, -1.0, 0.5];
        let mut left = [0i32; 2];
        let mut right = [0i32; 2];
        let count = interleave_to_deinterleaved_i32(&interleaved, &mut left, &mut right);
        assert_eq!(count, 2);
        assert_eq!(left[0], 0);
        assert_eq!(left[1], i32::MIN);
        assert_eq!(right[0], i32::MAX);
    }

    #[test]
    fn test_deinterleave_f32() {
        let interleaved = [0.1, 0.2, 0.3, 0.4];
        let mut left = [0.0; 2];
        let mut right = [0.0; 2];
        let count = interleave_to_deinterleaved_f32(&interleaved, &mut left, &mut right);
        assert_eq!(count, 2);
        assert_eq!(left, [0.1, 0.3]);
        assert_eq!(right, [0.2, 0.4]);
    }

    #[test]
    fn test_deinterleave_i24_packed() {
        let interleaved = [0.0, 1.0];
        let mut left = [0u8; 3];
        let mut right = [0u8; 3];
        let count = interleave_to_deinterleaved_i24_packed(&interleaved, &mut left, &mut right);
        assert_eq!(count, 1);
        assert_eq!(left, [0, 0, 0]);
        // 8388607 in hex is 0x7FFFFF -> LE: [0xFF, 0xFF, 0x7F]
        assert_eq!(right, [0xFF, 0xFF, 0x7F]);
    }

    #[test]
    fn test_deinterleave_be_formats() {
        let interleaved = [0.0, 1.0, -1.0, 0.5];
        let mut left_16 = [0i16; 2];
        let mut right_16 = [0i16; 2];
        interleave_to_deinterleaved_i16_be(&interleaved, &mut left_16, &mut right_16);
        assert_eq!(left_16[0], 0i16.to_be());
        assert_eq!(left_16[1], i16::MIN.to_be());
        assert_eq!(right_16[0], i16::MAX.to_be());

        let mut left_24 = [0u8; 3];
        let mut right_24 = [0u8; 3];
        interleave_to_deinterleaved_i24_packed_be(&[0.0, 1.0], &mut left_24, &mut right_24);
        // 8388607 in hex is 0x007FFFFF -> 24-bit BE bytes: [0x7F, 0xFF, 0xFF]
        assert_eq!(right_24, [0x7F, 0xFF, 0xFF]);
    }

    #[test]
    fn test_asio_sample_format_properties() {
        assert!(AsioSampleFormat::Int16LSB.is_supported());
        assert!(AsioSampleFormat::Int16MSB.is_supported());
        assert!(AsioSampleFormat::Int24LSB.is_supported());
        assert!(AsioSampleFormat::Int32LSB.is_supported());
        assert!(AsioSampleFormat::Float32LSB.is_supported());
        assert!(!AsioSampleFormat::Unknown(99).is_supported());

        assert_eq!(AsioSampleFormat::Int16LSB.bytes_per_sample(), 2);
        assert_eq!(AsioSampleFormat::Int24LSB.bytes_per_sample(), 3);
        assert_eq!(AsioSampleFormat::Int32LSB.bytes_per_sample(), 4);
        assert_eq!(AsioSampleFormat::Unknown(99).bytes_per_sample(), 0);
    }
}
