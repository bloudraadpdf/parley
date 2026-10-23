// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::{
    FallbackKey, FamilyId, FamilyInfo, FamilyNameMap, FontInfo, GenericFamily, GenericFamilyMap,
    ScriptExt, scan,
};
use crate::source::SourcePathMap;
use alloc::format;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{null, null_mut};
use hashbrown::{HashMap, HashSet};
use objc2_core_foundation::{
    CFArray, CFData, CFDictionary, CFRange, CFRetained, CFString, CFURL, CFURLPathStyle,
};
use objc2_core_text::{
    CTFont, CTFontCollection, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromData,
    CTFontUIFontType, kCTFontURLAttribute,
};
use objc2_foundation::{
    NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
};
use read_fonts::types::NameId;
use std::path::{Path, PathBuf};

const DEFAULT_GENERIC_FAMILIES: &[(GenericFamily, &[&str])] = &[
    (GenericFamily::Serif, &["Times", "Times New Roman"]),
    (GenericFamily::SansSerif, &["Helvetica"]),
    (GenericFamily::Monospace, &["Courier", "Courier New"]),
    (GenericFamily::Cursive, &["Apple Chancery"]),
    (GenericFamily::Fantasy, &["Papyrus"]),
    (GenericFamily::SystemUi, &["System Font", ".SF NS"]),
    (GenericFamily::Emoji, &["Apple Color Emoji"]),
    (GenericFamily::Math, &["STIX Two Math"]),
];

pub(crate) struct SystemFonts {
    pub(crate) name_map: Arc<FamilyNameMap>,
    pub(crate) generic_families: Arc<GenericFamilyMap>,
    family_map: HashMap<FamilyId, FamilyInfo>,
    data_paths: SourcePathMap,
}

impl SystemFonts {
    pub(crate) fn new() -> Self {
        let scanned = scan_system_fonts().unwrap_or_default();
        let name_map = scanned.family_names;
        let mut generic_families = GenericFamilyMap::default();
        for (family, names) in DEFAULT_GENERIC_FAMILIES {
            generic_families.set(
                *family,
                names
                    .iter()
                    .filter_map(|name| name_map.get(name))
                    .map(|name| name.id()),
            );
        }
        Self {
            name_map: Arc::new(name_map),
            generic_families: Arc::new(generic_families),
            family_map: scanned.families,
            data_paths: scanned.data_paths,
        }
    }

    pub(crate) fn family(&mut self, id: FamilyId) -> Option<FamilyInfo> {
        self.family_map.get(&id).cloned()
    }

    pub(crate) fn fallback(&mut self, key: impl Into<FallbackKey>) -> Option<FamilyId> {
        let key = key.into();
        let sample = key.script().sample()?;
        let font = create_fallback_font_for_text(sample, key.locale_str(), false, None, &[])?;
        let family_name = unsafe { font.family_name() };
        self.name_map.get(&family_name.to_string()).map(|n| n.id())
    }

    pub(crate) fn fallback_font_for_text(
        &mut self,
        base: Option<(&[u8], u32)>,
        language: Option<&str>,
        text: &str,
        variations: &[([u8; 4], f32)],
    ) -> Option<FontInfo> {
        let font = create_fallback_font_for_text(text, language, false, base, variations)?;
        let descriptor = unsafe { font.font_descriptor() };
        let path = font_file_path(&descriptor)?;
        let postscript_name = unsafe { font.post_script_name() }.to_string();
        let source = self.data_paths.get_or_insert(&path);
        let mut selected = None;
        scan::scan_paths([path], 0, |face| {
            if face
                .english_or_first_name(NameId::POSTSCRIPT_NAME)
                .is_some_and(|name| name.chars().eq(postscript_name.chars()))
            {
                selected = FontInfo::from_font_ref(&face.font, source.clone(), face.index);
            }
        });
        selected
    }
}

/// Discover system fonts by combining CoreText enumeration with a directory scan of all
/// Library/Fonts paths, then index them through the shared scan pipeline.
fn scan_system_fonts() -> Option<scan::ScannedCollection> {
    // SAFETY: Calls into CoreText. If anything fails we return None and use the fallback scan.
    let collection = unsafe { CTFontCollection::from_available_fonts(None) };
    let descriptors = unsafe { collection.matching_font_descriptors()? };
    let descriptors: CFRetained<CFArray<CTFontDescriptor>> =
        unsafe { CFRetained::cast_unchecked(descriptors) };

    // Collect unique font file paths to avoid redundant scanning.
    let mut paths: HashSet<PathBuf> = HashSet::new();
    for index in 0..descriptors.len() {
        let Some(descriptor) = descriptors.get(index) else {
            continue;
        };

        let Some(path) = font_file_path(&descriptor) else {
            continue;
        };
        if path.exists() {
            paths.insert(path);
        }
    }

    // Apple hides certain fonts from CTFontCollection (notably SFNS.ttf, the San Francisco
    // system UI font). Scanning Library/Fonts directories catches what CoreText omits.
    paths.extend(library_font_files());

    if paths.is_empty() {
        return None;
    }

    Some(scan::ScannedCollection::from_paths(paths.iter(), 0))
}

fn font_file_path(descriptor: &CTFontDescriptor) -> Option<PathBuf> {
    let url = unsafe { descriptor.attribute(kCTFontURLAttribute) }?
        .downcast::<CFURL>()
        .ok()?;
    Some(PathBuf::from(
        url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)?
            .to_string(),
    ))
}

fn library_font_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in NSSearchPathForDirectoriesInDomains(
        NSSearchPathDirectory::LibraryDirectory,
        NSSearchPathDomainMask::AllDomainsMask,
        true,
    ) {
        let font_dir = PathBuf::from(format!("{dir}/Fonts"));
        if font_dir.is_dir() {
            collect_files(&font_dir, 8, 0, &mut files);
        }
    }
    files
}

fn collect_files(dir: &Path, max_depth: u32, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if depth < max_depth {
                collect_files(&path, max_depth, depth + 1, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn create_base_font(prefer_ui_font: bool) -> CFRetained<CTFont> {
    if prefer_ui_font {
        if let Some(font) =
            unsafe { CTFont::new_ui_font_for_language(CTFontUIFontType::System, 0.0, None) }
        {
            return font;
        }
    }
    unsafe {
        let attrs = CFDictionary::new(None, null_mut(), null_mut(), 0, null(), null());
        let desc = CTFontDescriptor::with_attributes(&attrs.unwrap());
        CTFont::with_font_descriptor(&desc, 0.0, null())
    }
}

fn create_fallback_font_for_text(
    text: &str,
    locale: Option<&str>,
    prefer_ui_font: bool,
    base: Option<(&[u8], u32)>,
    variations: &[([u8; 4], f32)],
) -> Option<CFRetained<CTFont>> {
    let text = CFString::from_str(text);
    let text_range = CFRange {
        location: 0,
        length: text.length(),
    };
    let locale = locale.map(CFString::from_str);
    let base_font = if let Some((bytes, index)) = base {
        let data = CFData::from_bytes(bytes);
        // CoreText returns only font descriptors in this array.
        let descriptors: CFRetained<CFArray<CTFontDescriptor>> = unsafe {
            CFRetained::cast_unchecked(CTFontManagerCreateFontDescriptorsFromData(&data))
        };
        let mut descriptor = descriptors.get(index as usize)?;
        for (tag, value) in variations {
            descriptor = unsafe {
                descriptor.copy_with_variation(
                    &objc2_core_foundation::CFNumber::new_i32(i32::from_be_bytes(*tag)),
                    f64::from(*value),
                )
            };
        }
        unsafe { CTFont::with_font_descriptor(&descriptor, 0.0, null()) }
    } else {
        create_base_font(prefer_ui_font)
    };
    let font = unsafe {
        if let Some(locale) = locale {
            CTFont::for_string_with_language(&base_font, &text, text_range, Some(&locale))
        } else {
            CTFont::for_string(&base_font, &text, text_range)
        }
    };
    Some(font)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Collection, CollectionOptions, QueryStatus, SourceCache};
    use alloc::string::String;
    use read_fonts::types::NameId;

    #[test]
    fn text_fallback_preserves_a_variable_base_instance() {
        let mut collection = Collection::new(CollectionOptions::default());
        let mut cache = SourceCache::default();
        let base = {
            let mut query = collection.query(&mut cache);
            query.set_families([GenericFamily::SystemUi]);
            query.set_attributes(
                crate::Attributes::new(
                    crate::FontWidth::NORMAL,
                    crate::FontStyle::Normal,
                    crate::FontWeight::BOLD,
                ),
                crate::FontStyleSynthesis::Allowed,
            );
            let mut selected = None;
            query.matches_with(|font| {
                selected = Some(font.clone());
                QueryStatus::Stop
            });
            selected.expect("the installed variable system face")
        };
        assert!(!base.synthesis.variation_settings().is_empty());
        let descriptors = unsafe {
            CTFontManagerCreateFontDescriptorsFromData(&CFData::from_bytes(base.blob.as_ref()))
        };
        let descriptors: CFRetained<CFArray<CTFontDescriptor>> =
            unsafe { CFRetained::cast_unchecked(descriptors) };
        let mut descriptor = descriptors.get(base.index as usize).expect("base face");
        for (tag, value) in base.synthesis.variation_settings() {
            descriptor = unsafe {
                descriptor.copy_with_variation(
                    &objc2_core_foundation::CFNumber::new_i32(i32::from_be_bytes(
                        tag.to_be_bytes(),
                    )),
                    f64::from(*value),
                )
            };
        }
        let native_base = unsafe { CTFont::with_font_descriptor(&descriptor, 0.0, null()) };
        let text = CFString::from_str("⑴");
        let native = unsafe {
            CTFont::for_string_with_language(
                &native_base,
                &text,
                CFRange {
                    location: 0,
                    length: text.length(),
                },
                Some(&CFString::from_str("en")),
            )
        };
        let variations: Vec<_> = base
            .synthesis
            .variation_settings()
            .iter()
            .map(|(tag, value)| (tag.to_be_bytes(), *value))
            .collect();
        let actual = create_fallback_font_for_text(
            "⑴",
            Some("en"),
            false,
            Some((base.blob.as_ref(), base.index)),
            &variations,
        )
        .expect("fallback from variable source");
        assert_eq!(
            unsafe { actual.post_script_name() }.to_string(),
            unsafe { native.post_script_name() }.to_string()
        );
    }

    #[test]
    fn text_fallback_preserves_the_native_face() {
        let mut collection = Collection::new(CollectionOptions::default());
        let mut cache = SourceCache::default();
        let base = {
            let mut query = collection.query(&mut cache);
            query.set_families(["Helvetica Neue"]);
            let mut selected = None;
            query.matches_with(|font| {
                selected = Some(font.clone());
                QueryStatus::Stop
            });
            selected.expect("the installed base face")
        };
        let native = create_fallback_font_for_text(
            "⑴",
            Some("en"),
            false,
            Some((base.blob.as_ref(), base.index)),
            &[],
        )
        .expect("native fallback");
        let expected = unsafe { native.post_script_name() }.to_string();
        let selected = collection
            .system_fallback_for_text(Some((base.blob.as_ref(), base.index)), Some("en"), "⑴", &[])
            .expect("native source face");
        let data = selected
            .load(Some(&mut cache))
            .expect("selected source bytes");
        let mut actual = None;
        scan::scan_memory(data.as_ref(), |source| {
            if source.index == selected.index() {
                actual = source
                    .english_or_first_name(NameId::POSTSCRIPT_NAME)
                    .map(|name| name.chars().collect::<String>());
            }
        });
        assert_eq!(actual.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn base_collection_face_index_preserves_the_source_face() {
        let mut collection = Collection::new(CollectionOptions::default());
        let mut cache = SourceCache::default();
        let mut query = collection.query(&mut cache);
        query.set_families([GenericFamily::Serif]);
        let mut data = None;
        query.matches_with(|font| {
            data = Some(font.blob.clone());
            QueryStatus::Stop
        });
        let data = data.expect("the installed serif collection");
        let mut faces = 0;
        scan::scan_memory(data.as_ref(), |source| {
            let expected: String = source
                .english_or_first_name(NameId::POSTSCRIPT_NAME)
                .expect("the source PostScript name")
                .chars()
                .collect();
            let font = create_fallback_font_for_text(
                "A",
                None,
                false,
                Some((data.as_ref(), source.index)),
                &[],
            )
            .expect("CoreText must select the requested source face");
            assert_eq!(
                unsafe { font.post_script_name() }.to_string(),
                expected,
                "face {}",
                source.index
            );
            faces += 1;
        });
        assert!(faces > 1, "the native fixture must exercise a collection");
    }
}
