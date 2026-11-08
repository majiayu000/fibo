// Pictures for the composer: taken or picked, shrunk to what an Agent reads well and what
// fits in one line of the protocol, then sent as base64 for the computer to keep.
import { ImageManipulator, SaveFormat } from 'expo-image-manipulator';
import * as ImagePicker from 'expo-image-picker';
import { fit } from './fit';

export type Picture = { uri: string; base64: string };

/** A line is at most 1 MiB; base64 makes 4 of every 3 bytes, and the rest of the line needs room. */
export const MAX_BASE64 = 900_000;
/** At most this many in one message; the computer refuses more. */
export const MAX_PICTURES = 6;
/** Longest side, then the quality: tried in turn until it fits. 1568 is the most Claude reads at full detail. */
const STEPS: [number, number][] = [
  [1568, 0.75],
  [1280, 0.6],
  [960, 0.5],
];

async function shrink(asset: ImagePicker.ImagePickerAsset): Promise<Picture> {
  for (const [side, compress] of STEPS) {
    const ctx = ImageManipulator.manipulate(asset.uri);
    const size = fit(asset.width, asset.height, side);
    if (size) ctx.resize(size);
    const image = await ctx.renderAsync();
    const out = await image.saveAsync({ format: SaveFormat.JPEG, compress, base64: true });
    if (out.base64 && out.base64.length <= MAX_BASE64) return { uri: out.uri, base64: out.base64 };
  }
  throw new Error('这张图压缩后还是太大，换一张');
}

/**
 * Takes a photo or picks up to `room` from the library: null when the person backed out.
 * Throws with words to show when the permission is refused.
 */
export async function pick(from: 'camera' | 'library', room: number): Promise<Picture[] | null> {
  if (from === 'camera') {
    const p = await ImagePicker.requestCameraPermissionsAsync();
    if (!p.granted) throw new Error('没有相机权限：到系统设置里给 Mooring 打开');
  }
  const options: ImagePicker.ImagePickerOptions = {
    mediaTypes: 'images',
    quality: 1,
    allowsMultipleSelection: from === 'library' && room > 1,
    selectionLimit: room,
  };
  const r = from === 'camera' ? await ImagePicker.launchCameraAsync(options) : await ImagePicker.launchImageLibraryAsync(options);
  if (r.canceled || !r.assets?.length) return null;
  const out: Picture[] = [];
  for (const a of r.assets.slice(0, room)) out.push(await shrink(a));
  return out;
}
