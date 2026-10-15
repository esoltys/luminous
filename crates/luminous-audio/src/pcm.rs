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

#[inline]
pub fn f32_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped >= 1.0 {
        i16::MAX
    } else if clamped <= -1.0 {
        i16::MIN
    } else {
        (clamped * 32767.0).round() as i16
    }
}

#[inline]
pub fn f32_to_i32(sample: f32) -> i32 {
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped >= 1.0 {
        i32::MAX
    } else if clamped <= -1.0 {
        i32::MIN
    } else {
        (clamped * 2147483647.0).round() as i32
    }
}

#[inline]
pub fn f32_to_i24_in_i32(sample: f32) -> i32 {
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped >= 1.0 {
        8388607
    } else if clamped <= -1.0 {
        -8388608
    } else {
        (clamped * 8388607.0).round() as i32
    }
}

#[inline]
pub fn f32_to_i24_packed(sample: f32) -> [u8; 3] {
    let val = f32_to_i24_in_i32(sample);
    let bytes = val.to_le_bytes();
    [bytes[0], bytes[1], bytes[2]]
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
        AsioSampleFormat::Int32LSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i32>(), frames);
            interleave_to_deinterleaved_i32(interleaved, left, right);
        }
        AsioSampleFormat::Int32LSB24 => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i32>(), frames);
            interleave_to_deinterleaved_i32_24(interleaved, left, right);
        }
        AsioSampleFormat::Int24LSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr, frames * 3);
            let right = std::slice::from_raw_parts_mut(right_ptr, frames * 3);
            interleave_to_deinterleaved_i24_packed(interleaved, left, right);
        }
        AsioSampleFormat::Float32LSB => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<f32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<f32>(), frames);
            interleave_to_deinterleaved_f32(interleaved, left, right);
        }
        _ => {
            let left = std::slice::from_raw_parts_mut(left_ptr.cast::<i32>(), frames);
            let right = std::slice::from_raw_parts_mut(right_ptr.cast::<i32>(), frames);
            interleave_to_deinterleaved_i32(interleaved, left, right);
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
}
