//! Tags and cover art on prepared files: carrying the source's tags over to
//! a converted file, and downscaling embedded art.

use crate::Error;
use image::codecs::jpeg::JpegEncoder;
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture};
use lofty::probe::Probe;
use lofty::tag::Tag;
use std::io::Cursor;
use std::path::Path;

/// Downscaled art is saved as JPEG at this quality: small, and what every
/// player decodes.
const JPEG_QUALITY: u8 = 88;

/// Width and height of encoded image data, from its header alone.
pub(crate) fn dimensions(data: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// `picture` with its longer side at most `max_px`, re-encoded as JPEG; the
/// same picture when it is already that small or can't be decoded (a
/// player is better served by large art than none).
fn shrink(picture: &Picture, max_px: u32) -> Picture {
    let fits = dimensions(picture.data()).is_none_or(|(w, h)| w.max(h) <= max_px);
    if fits {
        return picture.clone();
    }
    let Ok(image) = image::load_from_memory(picture.data()) else {
        return picture.clone();
    };
    let small = image.resize(max_px, max_px, image::imageops::FilterType::Lanczos3);
    let mut jpeg = Vec::new();
    if JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
        .encode_image(&small.to_rgb8())
        .is_err()
    {
        return picture.clone();
    }
    let mut builder = Picture::unchecked(jpeg)
        .pic_type(picture.pic_type())
        .mime_type(MimeType::Jpeg);
    if let Some(description) = picture.description() {
        builder = builder.description(description.to_string());
    }
    builder.build()
}

fn read(path: &Path) -> Result<lofty::file::TaggedFile, Error> {
    Probe::open(path)
        .map_err(Error::at(path))?
        .options(ParseOptions::new().read_cover_art(true))
        .guess_file_type()
        .map_err(Error::at(path))?
        .read()
        .map_err(Error::at(path))
}

/// Write `source`'s tags and art into `converted`, a file in another format,
/// with art above `max_px` downscaled. Fields the target format can't hold
/// are dropped.
pub(crate) fn copy_tags(source: &Path, converted: &Path, max_px: Option<u32>) -> Result<(), Error> {
    let from = read(source)?;
    let Some(from_tag) = from.primary_tag().or_else(|| from.first_tag()) else {
        return Ok(());
    };
    let mut to = read(converted)?;
    let mut tag = Tag::new(to.primary_tag_type());
    for item in from_tag.items() {
        tag.push(item.clone());
    }
    for picture in from_tag.pictures() {
        tag.push_picture(match max_px {
            Some(max_px) => shrink(picture, max_px),
            None => picture.clone(),
        });
    }
    to.insert_tag(tag);
    to.save_to_path(converted, WriteOptions::default())
        .map_err(Error::at(converted))
}

/// Downscale every picture in `path`'s tags above `max_px`, in place. The
/// audio is not touched.
pub(crate) fn shrink_in_place(path: &Path, max_px: u32) -> Result<(), Error> {
    let mut file = read(path)?;
    let tag_types: Vec<_> = file.tags().iter().map(|t| t.tag_type()).collect();
    for tag_type in tag_types {
        let Some(tag) = file.tag_mut(tag_type) else {
            continue;
        };
        let pictures: Vec<Picture> = tag.pictures().iter().map(|p| shrink(p, max_px)).collect();
        for (i, picture) in pictures.into_iter().enumerate() {
            tag.set_picture(i, picture);
        }
    }
    file.save_to_path(path, WriteOptions::default())
        .map_err(Error::at(path))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `w` × `h` JPEG.
    pub(crate) fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let image = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 128]));
        let mut data = Vec::new();
        JpegEncoder::new(&mut data).encode_image(&image).unwrap();
        data
    }

    #[test]
    fn shrink_keeps_aspect_and_small_pictures() {
        let big = Picture::unchecked(jpeg(1200, 600))
            .pic_type(lofty::picture::PictureType::CoverFront)
            .build();
        let small = shrink(&big, 500);
        assert_eq!(dimensions(small.data()), Some((500, 250)));
        assert_eq!(small.pic_type(), lofty::picture::PictureType::CoverFront);
        assert_eq!(small.mime_type(), Some(&MimeType::Jpeg));

        assert_eq!(shrink(&small, 500), small);
    }

    #[test]
    fn undecodable_art_is_kept() {
        let junk = Picture::unchecked(vec![0xFF, 0xD8, 0xFF, 0xE0]).build();
        assert_eq!(shrink(&junk, 100), junk);
    }
}
