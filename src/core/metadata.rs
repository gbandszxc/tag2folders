//! 音频元数据提取（lofty）。

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::Tag;

use crate::core::AudioMetadata;

/// 兜底值（preview 的 missing_metadata 判断依赖这些确切字符串，勿改动）。
pub const FALLBACK_ARTIST: &str = "Unknown Artist";
pub const FALLBACK_ALBUM: &str = "Unknown Album";
pub const FALLBACK_TITLE: &str = "Unknown Title";
pub const FALLBACK_TRACK: &str = "0";
pub const FALLBACK_YEAR: &str = "Unknown Year";
pub const FALLBACK_GENRE: &str = "Unknown Genre";

/// 内部字段名 → 兜底值（对应 Python `_FALLBACKS`）。
fn fallback_for(field: &str) -> &'static str {
    match field {
        "artist" => FALLBACK_ARTIST,
        "album" => FALLBACK_ALBUM,
        "title" => FALLBACK_TITLE,
        "track" => FALLBACK_TRACK,
        "year" => FALLBACK_YEAR,
        "genre" => FALLBACK_GENRE,
        _ => "",
    }
}

/// RIFF LIST/INFO 子块 ID → 内部字段名（对应 Python `_RIFF_INFO_TO_FIELD`）。
/// ffmpeg 与许多 DAW 用这些 4 字符 INFO ID 而非 ID3 帧嵌入元数据。
const RIFF_INFO_TO_FIELD: &[(&str, &str)] = &[
    ("INAM", "title"),
    ("IART", "artist"),
    ("IPRD", "album"),
    ("IPRT", "track"),
    ("ICRD", "year"),
    ("IGNR", "genre"),
];

/// 按字段取兜底值辅助：内部字段名 → 候选 ItemKey 列表。
///
/// 对应 Python `_TAG_TO_FIELD` + `_EASY_TO_ID3_FRAME` 的查找链：
/// - artist：TrackArtist，无则 AlbumArtist
/// - date：Year（ID3v2.3 TYER），无则 RecordingDate（ID3v2.4 TDRC / RIFF ICRD）
fn field_keys(field: &str) -> &'static [ItemKey] {
    match field {
        "artist" => &[ItemKey::TrackArtist, ItemKey::AlbumArtist],
        "album" => &[ItemKey::AlbumTitle],
        "title" => &[ItemKey::TrackTitle],
        "track" => &[ItemKey::TrackNumber],
        "year" => &[ItemKey::Year, ItemKey::RecordingDate],
        "genre" => &[ItemKey::Genre],
        _ => &[],
    }
}

/// 从 lofty 标签按键顺序取第一条 trim 后非空的文本值。
fn tag_text(tag: Option<&Tag>, keys: &[ItemKey]) -> Option<String> {
    let tag = tag?;
    for key in keys {
        let Some(item) = tag.get(key) else {
            continue;
        };
        // 对应 Python str(raw[0]).strip()：文本（或定位符）取首条，去首尾空白
        let text = item
            .value()
            .text()
            .or_else(|| item.value().locator())
            .map(str::trim);
        if let Some(text) = text {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// 提取单个音频文件的元数据。无法解析时返回 readable=false + error 信息，
/// 不返回 Err。标题缺失时用文件名主干；音轨号 "3/12" 规范化为 "3"。
pub fn extract_metadata(file_path: &str) -> AudioMetadata {
    let path = Path::new(file_path);
    // Python: ext = path.suffix.lstrip(".").lower()
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Python: MutagenFile(file_path, easy=True)；解析失败 → readable=false + 全兜底
    let parsed = Probe::open(path).and_then(|probe| probe.read());
    let tagged_file = match parsed {
        Ok(tagged_file) => tagged_file,
        Err(err) => {
            return AudioMetadata {
                path: file_path.to_string(),
                ext,
                artist: FALLBACK_ARTIST.to_string(),
                album: FALLBACK_ALBUM.to_string(),
                title: stem,
                track: FALLBACK_TRACK.to_string(),
                year: FALLBACK_YEAR.to_string(),
                genre: FALLBACK_GENRE.to_string(),
                readable: false,
                error: err.to_string(),
            };
        }
    };

    // 主标签优先（WAV 的主标签类型为 Id3v2），否则取第一个可用标签
    // （仅有 RIFF LIST/INFO 的 WAV 落到这里，lofty 会将其解析为 RiffInfo 标签）。
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    // WAV 文件预解析 RIFF LIST/INFO 块：lofty 未覆盖的字段用它兜底
    // （对应 Python 对 wav 的 _parse_riff_info 预解析）。
    let riff_info = if ext == "wav" {
        parse_riff_info(file_path)
    } else {
        HashMap::new()
    };

    /// 三级查找（对应 Python `_get`）：lofty 标签 → RIFF LIST/INFO → 兜底值。
    fn get(tag: Option<&Tag>, riff_info: &HashMap<String, String>, field: &str) -> String {
        if let Some(value) = tag_text(tag, field_keys(field)) {
            return value;
        }
        if let Some(value) = riff_info.get(field) {
            if !value.is_empty() {
                return value.clone();
            }
        }
        fallback_for(field).to_string()
    }

    let artist = get(tag, &riff_info, "artist");
    let album = get(tag, &riff_info, "album");
    let mut title = get(tag, &riff_info, "title");
    let track_raw = get(tag, &riff_info, "track");
    // 规范化 "3/12" → "3"（lofty 对 ID3 TRCK 已拆分，Vorbis/RIFF 原样保留）
    let track = track_raw.split('/').next().unwrap_or(&track_raw).to_string();
    let year = get(tag, &riff_info, "year");
    let genre = get(tag, &riff_info, "genre");

    // 完全没有标题标签时用文件名主干（对应 Python title == fallback 时替换）
    if title == FALLBACK_TITLE {
        title = stem;
    }

    AudioMetadata {
        path: file_path.to_string(),
        ext,
        artist,
        album,
        title,
        track,
        year,
        genre,
        readable: true,
        error: String::new(),
    }
}

/// 批量提取。
pub fn extract_metadata_batch(file_paths: &[String]) -> Vec<AudioMetadata> {
    file_paths.iter().map(|p| extract_metadata(p)).collect()
}

/// 解析 WAV 文件的 RIFF LIST/INFO 块，返回 字段名 → 值 映射
/// （如 {"artist": "Pink Floyd", ...}）。无 INFO 块或解析失败返回空表。
///
/// 对应 Python `_parse_riff_info`。RIFF 结构：
/// ```text
/// 'RIFF' <4 字节 LE 尺寸> 'WAVE'
///   <chunk-id> <4 字节 LE 尺寸> <data> ...
/// LIST 块：
///   'LIST' <4 字节 LE 尺寸> 'INFO'
///     <4 字符 info-id> <4 字节 LE 尺寸> <NUL 结尾字符串，偶数字节对齐> ...
/// ```
fn parse_riff_info(file_path: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();

    let Ok(mut fh) = File::open(file_path) else {
        return result;
    };
    // Python 用 fh.tell() 驱动循环；这里显式跟踪读取位置。
    let mut pos: u64 = 0;

    let mut header = [0u8; 12];
    if fh.read_exact(&mut header).is_err() {
        return result;
    }
    pos += 12;
    if &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return result;
    }

    let riff_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
    let end_pos = 8 + riff_size; // RIFF 载荷的绝对结束位置

    while pos < end_pos {
        let mut chunk_header = [0u8; 8];
        if fh.read_exact(&mut chunk_header).is_err() {
            break;
        }
        pos += 8;
        let chunk_id = String::from_utf8_lossy(&chunk_header[..4]).into_owned();
        let chunk_size =
            u32::from_le_bytes([chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7]])
                as u64;
        let chunk_start = pos;

        if chunk_id == "LIST" {
            let mut list_type = [0u8; 4];
            if fh.read_exact(&mut list_type).is_err() {
                break;
            }
            pos += 4;
            if &list_type == b"INFO" {
                let info_end = chunk_start + chunk_size;
                while pos < info_end {
                    let mut tag_header = [0u8; 8];
                    if fh.read_exact(&mut tag_header).is_err() {
                        break;
                    }
                    pos += 8;
                    let tag_id = String::from_utf8_lossy(&tag_header[..4]).into_owned();
                    let tag_size = u32::from_le_bytes([
                        tag_header[4],
                        tag_header[5],
                        tag_header[6],
                        tag_header[7],
                    ]) as usize;
                    // 对应 Python fh.read(tag_size)：容忍短读
                    let mut tag_data = vec![0u8; tag_size];
                    let read_len = fh.read(&mut tag_data).unwrap_or(0);
                    tag_data.truncate(read_len);
                    pos += read_len as u64;
                    // 跳过奇数尺寸后的填充字节
                    if tag_size % 2 == 1 {
                        let mut pad = [0u8; 1];
                        let _ = fh.read(&mut pad);
                        pos += 1;
                    }
                    let Some(field) = riff_info_field(&tag_id) else {
                        continue;
                    };
                    // 去掉尾部 NUL；先尝试 UTF-8，失败回退 Latin-1
                    //（Windows 上常见的 ANSI 编码文件）
                    let raw = strip_trailing_nuls(&tag_data);
                    let value = match std::str::from_utf8(raw) {
                        Ok(s) => s.trim().to_string(),
                        Err(_) => raw.iter().map(|&b| b as char).collect::<String>().trim().to_string(),
                    };
                    if !value.is_empty() {
                        result.insert(field.to_string(), value);
                    }
                }
            }
            // 跳过 LIST 块剩余数据（类型 4 字节已读），补齐偶数字节填充，
            // 使外层循环从下一个块对齐开始（对应回归测试：奇数尺寸 LIST 块）。
            let seek_to = chunk_start + chunk_size + (chunk_size % 2);
            if fh.seek(SeekFrom::Start(seek_to)).is_err() {
                break;
            }
            pos = seek_to;
        } else {
            // 跳过未知块；补齐到偶数边界
            let seek_to = chunk_start + chunk_size + (chunk_size % 2);
            if fh.seek(SeekFrom::Start(seek_to)).is_err() {
                break;
            }
            pos = seek_to;
        }
    }

    result
}

/// RIFF INFO ID → 内部字段名查表（对应 Python `_RIFF_INFO_TO_FIELD.get(tag_id)`）。
fn riff_info_field(tag_id: &str) -> Option<&'static str> {
    RIFF_INFO_TO_FIELD
        .iter()
        .find(|(id, _)| *id == tag_id)
        .map(|(_, field)| *field)
}

/// 去掉尾部 NUL 字节（对应 Python `raw.rstrip(b"\x00")`）。
fn strip_trailing_nuls(data: &[u8]) -> &[u8] {
    let mut end = data.len();
    while end > 0 && data[end - 1] == 0 {
        end -= 1;
    }
    &data[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // ── 测试基建 ──────────────────────────────────────────────────────────────

    /// 创建唯一临时目录，返回其路径（调用方负责清理）。
    fn make_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("t2f_metadata_{tag}_{}_{}", std::process::id(), uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 构造最小合法 WAV 文件字节（1 声道 / 16 位 / 44100 Hz / 100 帧零数据），
    /// 等价 Python 测试中 `wave` 模块写出的文件。
    fn minimal_wav_bytes() -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
        fmt.extend_from_slice(&1u16.to_le_bytes()); // 单声道
        fmt.extend_from_slice(&44100u32.to_le_bytes()); // 采样率
        fmt.extend_from_slice(&88200u32.to_le_bytes()); // 字节率
        fmt.extend_from_slice(&2u16.to_le_bytes()); // 块对齐
        fmt.extend_from_slice(&16u16.to_le_bytes()); // 位深

        let data = [0u8; 100];

        let mut body = Vec::new();
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(&data);

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(&body);
        out
    }

    /// 追加 RIFF 块并更新 RIFF 总尺寸（等价 Python 测试中的 r+b 追加逻辑）。
    fn append_chunk(wav: &mut Vec<u8>, chunk: &[u8]) {
        wav.extend_from_slice(chunk);
        let new_riff_size = (wav.len() - 8) as u32;
        wav[4..8].copy_from_slice(&new_riff_size.to_le_bytes());
    }

    /// 构造 LIST/INFO 块（ffmpeg 风格元数据）。
    fn list_info_chunk(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut info = Vec::new();
        info.extend_from_slice(b"INFO");
        for (id, value) in entries {
            let mut data = value.to_vec();
            data.push(0); // NUL 结尾
            info.extend_from_slice(id);
            info.extend_from_slice(&(data.len() as u32).to_le_bytes());
            info.extend_from_slice(&data);
            if data.len() % 2 == 1 {
                info.push(0); // 偶数字节填充
            }
        }
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"LIST");
        chunk.extend_from_slice(&(info.len() as u32).to_le_bytes());
        chunk.extend_from_slice(&info);
        chunk
    }

    /// 构造嵌入 'id3 ' 块的 ID3v2.4 标签字节（等价 mutagen WAVE 接口写出的形式）。
    /// 帧内容小（< 128 字节），同步安全整数与普通 BE u32 编码相同。
    fn id3_chunk(frames: &[(&[u8; 4], &str)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, text) in frames {
            let mut content = vec![3]; // 编码 3 = UTF-8
            content.extend_from_slice(text.as_bytes());
            body.extend_from_slice(id.as_slice());
            body.extend_from_slice(&(content.len() as u32).to_be_bytes());
            body.extend_from_slice(&[0, 0]); // 帧标志
            body.extend_from_slice(&content);
        }
        let mut tag = Vec::new();
        tag.extend_from_slice(b"ID3");
        tag.extend_from_slice(&[4, 0]); // 版本 2.4
        tag.extend_from_slice(&[0]); // 标志
        tag.extend_from_slice(&(body.len() as u32).to_be_bytes()); // 同步安全总尺寸
        tag.extend_from_slice(&body);

        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"id3 ");
        chunk.extend_from_slice(&(tag.len() as u32).to_le_bytes());
        chunk.extend_from_slice(&tag);
        chunk
    }

    // ── extract_metadata 单元测试 ─────────────────

    /// 空 .mp3 文件应返回 readable=false 且全部兜底值。
    #[test]
    fn test_extract_metadata_empty_file_returns_fallbacks() {
        let dir = make_temp_dir("empty_mp3");
        let path = dir.join("empty.mp3");
        fs::write(&path, b"").unwrap();

        let m = extract_metadata(path.to_str().unwrap());
        assert!(!m.readable);
        assert_eq!(m.artist, "Unknown Artist");
        assert_eq!(m.album, "Unknown Album");
        assert_eq!(m.year, "Unknown Year");
        assert_eq!(m.genre, "Unknown Genre");
        assert_eq!(m.track, "0");

        fs::remove_dir_all(&dir).unwrap();
    }

    /// 不存在的文件应返回 readable=false，不 panic。
    #[test]
    fn test_extract_metadata_nonexistent_file_returns_fallbacks() {
        let missing = std::env::temp_dir().join("does_not_exist_xyzabc.mp3");
        let m = extract_metadata(missing.to_str().unwrap());
        assert!(!m.readable);
        assert_eq!(m.artist, "Unknown Artist");
        assert_eq!(m.year, "Unknown Year");
    }

    /// 回归：缺失 date/year 标签不得引发任何错误。
    #[test]
    fn test_extract_metadata_no_key_error_on_missing_date() {
        let dir = make_temp_dir("missing_date");
        let path = dir.join("nodate.mp3");
        fs::write(&path, b"").unwrap();

        let m = extract_metadata(path.to_str().unwrap());
        assert!(matches!(m.year.as_str(), "Unknown Year" | "0" | ""));

        fs::remove_dir_all(&dir).unwrap();
    }

    /// 嵌入 ID3 标签（'id3 ' RIFF 块）的 WAV 必须返回正确字段值而非 Unknown *。
    #[test]
    fn test_extract_metadata_wav_with_id3_tags() {
        let dir = make_temp_dir("wav_id3");
        let path = dir.join("tagged.wav");
        let mut wav = minimal_wav_bytes();
        append_chunk(
            &mut wav,
            &id3_chunk(&[
                (b"TIT2", "Comfortably Numb"),
                (b"TPE1", "Pink Floyd"),
                (b"TALB", "The Wall"),
                (b"TRCK", "6/26"),
                (b"TDRC", "1979"),
                (b"TCON", "Rock"),
            ]),
        );
        fs::write(&path, &wav).unwrap();

        let m = extract_metadata(path.to_str().unwrap());
        assert!(m.readable, "WAV with ID3 tags must be readable; error: {}", m.error);
        assert_eq!(m.artist, "Pink Floyd");
        assert_eq!(m.album, "The Wall");
        assert_eq!(m.title, "Comfortably Numb");
        assert_eq!(m.track, "6", "Expected '6' after stripping '6/26', got {:?}", m.track);
        assert_eq!(m.year, "1979");
        assert_eq!(m.genre, "Rock");

        fs::remove_dir_all(&dir).unwrap();
    }

    /// 无标签 WAV 返回兜底值且仍可读（不 panic 即可，与 Python 测试一致）。
    #[test]
    fn test_extract_metadata_wav_no_tags_returns_fallbacks() {
        let dir = make_temp_dir("wav_plain");
        let path = dir.join("plain.wav");
        fs::write(&path, minimal_wav_bytes()).unwrap();

        let m = extract_metadata(path.to_str().unwrap());
        // 无标签 WAV：有效音频 → readable=true + 兜底字段
        assert!(m.artist == "Unknown Artist" || m.artist.is_empty() || !m.readable);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// ffmpeg 风格（RIFF LIST/INFO 子块）元数据的 WAV，第三级兜底必须命中。
    #[test]
    fn test_extract_metadata_wav_with_riff_info_chunk() {
        let dir = make_temp_dir("wav_info");
        let path = dir.join("info.wav");
        let mut wav = minimal_wav_bytes();
        append_chunk(
            &mut wav,
            &list_info_chunk(&[
                (b"INAM", b"Comfortably Numb"),
                (b"IART", b"Pink Floyd"),
                (b"IPRD", b"The Wall"),
                (b"IPRT", b"6"),
                (b"ICRD", b"1979"),
                (b"IGNR", b"Rock"),
            ]),
        );
        fs::write(&path, &wav).unwrap();

        let m = extract_metadata(path.to_str().unwrap());
        assert!(m.readable, "WAV with LIST/INFO chunk must be readable; error: {}", m.error);
        assert_eq!(m.artist, "Pink Floyd");
        assert_eq!(m.album, "The Wall");
        assert_eq!(m.title, "Comfortably Numb");
        assert_eq!(m.track, "6");
        assert_eq!(m.year, "1979");
        assert_eq!(m.genre, "Rock");

        fs::remove_dir_all(&dir).unwrap();
    }

    /// 奇数尺寸的非 INFO LIST 块（如 'adtl'）在 LIST/INFO 之前时，
    /// 解析器必须跳过填充字节才能在正确偏移找到 INFO 块。
    #[test]
    fn test_extract_metadata_wav_info_after_odd_size_list_chunk() {
        let dir = make_temp_dir("wav_adtl");
        let path = dir.join("adtl.wav");
        let mut wav = minimal_wav_bytes();

        // 奇数尺寸 LIST/adtl 块（5 字节载荷 → 需 1 个填充字节）
        let mut adtl_chunk = Vec::new();
        let adtl_payload = b"adtlx"; // "adtl" + 'x'，5 字节
        adtl_chunk.extend_from_slice(b"LIST");
        adtl_chunk.extend_from_slice(&(adtl_payload.len() as u32).to_le_bytes());
        adtl_chunk.extend_from_slice(adtl_payload);
        adtl_chunk.push(0); // 偶数字节填充
        append_chunk(&mut wav, &adtl_chunk);

        append_chunk(
            &mut wav,
            &list_info_chunk(&[
                (b"INAM", b"Comfortably Numb"),
                (b"IART", b"Pink Floyd"),
                (b"IPRD", b"The Wall"),
                (b"IPRT", b"6"),
                (b"ICRD", b"1979"),
                (b"IGNR", b"Rock"),
            ]),
        );
        fs::write(&path, &wav).unwrap();

        let m = extract_metadata(path.to_str().unwrap());
        assert!(m.readable, "WAV must be readable; error: {}", m.error);
        assert_eq!(
            m.artist, "Pink Floyd",
            "Expected 'Pink Floyd', got {:?}. Odd-sized LIST chunk padding may not be skipped correctly.",
            m.artist
        );
        assert_eq!(m.title, "Comfortably Numb");
        assert_eq!(m.track, "6");
        assert_eq!(m.year, "1979");

        fs::remove_dir_all(&dir).unwrap();
    }
}
