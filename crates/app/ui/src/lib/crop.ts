// Which part of the photo the video keeps. Mirrors the center crop in
// crates/core/src/builder.rs (`filter_chain`), as fractions of the photo.

export type Box = { left: number; top: number; width: number; height: number }

export function cropBox(photoW: number, photoH: number, videoW: number, videoH: number): Box {
  const photo = photoW / photoH
  const video = videoW / videoH
  if (photo > video) {
    const width = video / photo
    return { left: (1 - width) / 2, top: 0, width, height: 1 }
  }
  const height = photo / video
  return { left: 0, top: (1 - height) / 2, width: 1, height }
}

/** Below half a percent the crop is a rounding edge, not worth a frame. */
export function isCropped(box: Box): boolean {
  return box.width < 0.995 || box.height < 0.995
}
