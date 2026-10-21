//! NumPy `.npy` version 1.0, little-endian, C order. Enough to read and write the
//! arrays a project holds; Python opens them with `numpy.load`.

use crate::ProjectError;

pub trait Element: Copy + Sized {
    const DESCR: &'static str;
    const SIZE: usize;
    fn write_le(self, out: &mut Vec<u8>);
    fn read_le(bytes: &[u8]) -> Self;
}

macro_rules! element {
    ($t:ty, $descr:literal) => {
        impl Element for $t {
            const DESCR: &'static str = $descr;
            const SIZE: usize = std::mem::size_of::<$t>();
            fn write_le(self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn read_le(bytes: &[u8]) -> Self {
                <$t>::from_le_bytes(bytes.try_into().expect("element size"))
            }
        }
    };
}

element!(f32, "<f4");
element!(f64, "<f8");
element!(u32, "<u4");
element!(u16, "<u2");
element!(u8, "|u1");

const MAGIC: &[u8] = b"\x93NUMPY\x01\x00";

pub fn write<T: Element>(shape: &[usize], data: &[T]) -> Vec<u8> {
    assert_eq!(shape.iter().product::<usize>(), data.len(), "shape does not match data");
    let dims = match shape {
        [n] => format!("({n},)"),
        _ => format!("({})", shape.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")),
    };
    let mut header = format!("{{'descr': '{}', 'fortran_order': False, 'shape': {dims}, }}", T::DESCR);
    // magic + 2-byte length + header + newline, padded to a multiple of 64
    let unpadded = MAGIC.len() + 2 + header.len() + 1;
    header.push_str(&" ".repeat(unpadded.next_multiple_of(64) - unpadded));
    header.push('\n');

    let mut out = Vec::with_capacity(MAGIC.len() + 2 + header.len() + data.len() * T::SIZE);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(header.len() as u16).to_le_bytes());
    out.extend_from_slice(header.as_bytes());
    for &x in data {
        x.write_le(&mut out);
    }
    out
}

pub fn read<T: Element>(bytes: &[u8]) -> Result<(Vec<usize>, Vec<T>), ProjectError> {
    let bad = |why: &str| ProjectError::Malformed(format!("npy: {why}"));
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" {
        return Err(bad("missing magic"));
    }
    let (header_len, start) = match bytes[6] {
        1 => (u16::from_le_bytes([bytes[8], bytes[9]]) as usize, 10),
        2 | 3 if bytes.len() >= 12 => (u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize, 12),
        _ => return Err(bad("unsupported version")),
    };
    let header =
        std::str::from_utf8(bytes.get(start..start + header_len).ok_or_else(|| bad("short header"))?)
            .map_err(|_| bad("header is not text"))?;
    if !header.contains(&format!("'descr': '{}'", T::DESCR)) {
        return Err(bad(&format!("expected dtype {}, header is {header}", T::DESCR)));
    }
    if header.contains("'fortran_order': True") {
        return Err(bad("fortran order is not supported"));
    }
    let shape_txt =
        header.split("'shape': (").nth(1).and_then(|s| s.split(')').next()).ok_or_else(|| bad("no shape"))?;
    let shape: Vec<usize> = shape_txt
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().map_err(|_| bad("bad shape")))
        .collect::<Result<_, _>>()?;
    let n: usize = shape.iter().product();
    let body = &bytes[start + header_len..];
    if body.len() != n * T::SIZE {
        return Err(bad("data length does not match shape"));
    }
    Ok((shape, body.chunks_exact(T::SIZE).map(T::read_le).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_aligned_and_round_trips() {
        let data: Vec<f32> = (0..12).map(|i| i as f32 * 0.5).collect();
        let bytes = write(&[4, 3], &data);
        let header_len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        assert_eq!((10 + header_len) % 64, 0);
        let (shape, back) = read::<f32>(&bytes).unwrap();
        assert_eq!(shape, vec![4, 3]);
        assert_eq!(back, data);
    }

    #[test]
    fn one_dimensional_shape_has_trailing_comma() {
        let bytes = write(&[3], &[1u16, 2, 3]);
        let text = String::from_utf8_lossy(&bytes[10..]);
        assert!(text.contains("'shape': (3,)"));
        assert_eq!(read::<u16>(&bytes).unwrap().1, vec![1, 2, 3]);
    }

    #[test]
    fn wrong_dtype_is_refused() {
        let bytes = write(&[2], &[1u32, 2]);
        assert!(read::<f32>(&bytes).is_err());
    }
}
