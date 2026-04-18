use windows::core::Interface;
use winreg::RegKey;
use winreg::RegValue;
use winreg::enums::*;
use winreg::types::ToRegValue;

use crate::JP2WICBitmapDecoder;
use crate::guid::{JP2WINTHUMB_VENDOR_CLSID, guid_to_string};

const EXT: &str = ".jp2";

const PROGID: &str = "jp2winthumbfile";
const CONTENT_TYPE_KEY: &str = "Content Type";
const CONTENT_TYPE_VALUE: &str = "image/jp2";
const PERCEIVED_TYPE_KEY: &str = "PerceivedType";
const PERCEIVED_TYPE_VALUE: &str = "image";

const FRIENDLY_NAME: &str = "jp2-winthumb WIC Decoder";
const PROGID_FRIENDLY: &str = "JP2 File";

// CATID_WICBitmapDecoders — the WIC category our decoder joins.
const CATID_WIC_DECODERS: &str = "{7ED96837-96F0-4812-B211-F13C24117ED3}";

// CLSID_PhotoThumbnailProvider — Windows' stock thumbnail provider that
// uses any registered WIC decoder to render thumbnails.
const CLSID_PHOTO_THUMBNAIL_PROVIDER: &str = "{C7657C4A-9F68-40fa-A4DF-96BC08EB3551}";

// Standard shell image preview handler.
const CLSID_SHELL_IMAGE_PREVIEW: &str = "{FFE2A43C-56B9-4bf5-9A79-CC6D4285608A}";

fn register_clsid_base(
    module_path: &str,
    clsid: &windows::core::GUID,
) -> std::io::Result<RegKey> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let clsid_key = hkcr.open_subkey("CLSID")?;
    let (key, _) = clsid_key.create_subkey(guid_to_string(clsid))?;
    key.set_value("", &"jp2-winthumb")?;

    let (inproc, _) = key.create_subkey("InProcServer32")?;
    inproc.set_value("", &module_path)?;
    inproc.set_value("ThreadingModel", &"Both")?;

    Ok(key)
}

fn open_clsid(key: &str) -> std::io::Result<RegKey> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let clsid_key = hkcr.open_subkey("CLSID")?;
    clsid_key.open_subkey(key)
}

fn set_pattern(key: &RegKey, pattern: Vec<u8>) -> std::io::Result<()> {
    let len = pattern.len();
    key.set_value("Position", &0u32)?;
    key.set_value("Length", &(len as u32))?;
    key.set_raw_value(
        "Pattern",
        &RegValue {
            vtype: REG_BINARY,
            bytes: pattern,
        },
    )?;
    key.set_raw_value(
        "Mask",
        &RegValue {
            vtype: REG_BINARY,
            bytes: vec![0xff; len],
        },
    )?;
    Ok(())
}

fn register_clsid(module_path: &str) -> std::io::Result<()> {
    let wic_decoder_key = register_clsid_base(module_path, &JP2WICBitmapDecoder::CLSID)?;
    wic_decoder_key.set_value("FriendlyName", &FRIENDLY_NAME)?;
    wic_decoder_key.set_value("VendorGUID", &guid_to_string(&JP2WINTHUMB_VENDOR_CLSID))?;
    wic_decoder_key.set_value("MimeTypes", &CONTENT_TYPE_VALUE)?;
    wic_decoder_key.set_value("FileExtensions", &EXT)?;

    let (formats, _) = wic_decoder_key.create_subkey("Formats")?;
    formats.create_subkey(guid_to_string(
        &windows::Win32::Graphics::Imaging::GUID_WICPixelFormat32bppRGBA,
    ))?;

    // JP2 file signature box: the first 12 bytes of every JP2 file.
    //   LBox (4) = 0x0000000C
    //   TBox (4) = "jP  " (0x6A502020)
    //   Content  = 0x0D0A870A
    let (patterns, _) = wic_decoder_key.create_subkey("Patterns")?;
    let (p0, _) = patterns.create_subkey("0")?;
    set_pattern(
        &p0,
        vec![
            0x00, 0x00, 0x00, 0x0c, 0x6a, 0x50, 0x20, 0x20, 0x0d, 0x0a, 0x87, 0x0a,
        ],
    )?;

    // Register ourselves as a member of the WIC Bitmap Decoders category.
    let instances_key = open_clsid(CATID_WIC_DECODERS)?.open_subkey("Instance")?;
    let (instance_key, _) =
        instances_key.create_subkey(guid_to_string(&JP2WICBitmapDecoder::CLSID))?;
    instance_key.set_value("CLSID", &guid_to_string(&JP2WICBitmapDecoder::CLSID))?;
    instance_key.set_value("FriendlyName", &FRIENDLY_NAME)?;

    Ok(())
}

fn unregister_clsid() {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);

    hkcr.delete_subkey_all(format!(
        "CLSID\\{}",
        &guid_to_string(&JP2WICBitmapDecoder::CLSID)
    ))
    .ok();

    hkcr.delete_subkey_all(format!(
        "CLSID\\{}\\Instance\\{}",
        CATID_WIC_DECODERS,
        &guid_to_string(&JP2WICBitmapDecoder::CLSID)
    ))
    .ok();
}

fn create_expand_sz(value: &str) -> RegValue {
    RegValue {
        vtype: winreg::enums::REG_EXPAND_SZ,
        bytes: value.to_reg_value().bytes,
    }
}

fn register_property_list(system_ext_key: &RegKey) -> std::io::Result<()> {
    system_ext_key.set_value(
        "FullDetails",
        &"prop:System.PropGroup.Image;System.Image.Dimensions;System.Image.HorizontalSize;System.Image.VerticalSize;System.PropGroup.FileSystem;System.ItemNameDisplay;System.ItemType;System.ItemFolderPathDisplay;System.DateCreated;System.DateModified;System.Size;System.FileAttributes;System.OfflineAvailability;System.OfflineStatus;System.SharedWith;System.FileOwner;System.ComputerName",
    )?;
    system_ext_key.set_value(
        "PreviewDetails",
        &"prop:*System.Image.Dimensions;*System.Size;*System.OfflineAvailability;*System.OfflineStatus;*System.DateCreated;*System.DateModified;*System.DateAccessed;*System.SharedWith",
    )?;
    Ok(())
}

fn register_provider() -> std::io::Result<()> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let (ext_key, _) = hkcr.create_subkey(EXT)?;
    ext_key.set_value("", &PROGID)?;
    ext_key.set_value(CONTENT_TYPE_KEY, &CONTENT_TYPE_VALUE)?;
    ext_key.set_value(PERCEIVED_TYPE_KEY, &PERCEIVED_TYPE_VALUE)?;

    ext_key.create_subkey(format!("OpenWithProgids\\{}", PROGID))?;

    let (system_ext_key, _) = hkcr.create_subkey(format!("SystemFileAssociations\\{}", EXT))?;
    system_ext_key
        .create_subkey("ShellEx\\ContextMenuHandlers\\ShellImagePreview")?
        .0
        .set_value("", &CLSID_SHELL_IMAGE_PREVIEW)?;
    register_property_list(&system_ext_key)?;

    let (progid_key, _) = hkcr.create_subkey(PROGID)?;
    progid_key.set_value("", &PROGID_FRIENDLY)?;
    let (progid_shell_key, _) = progid_key.create_subkey("shell")?;
    let (open_key, _) = progid_shell_key.create_subkey("open")?;
    open_key.set_raw_value(
        "MuiVerb",
        &create_expand_sz("@%PROGRAMFILES%\\Windows Photo Viewer\\photoviewer.dll,-3043"),
    )?;
    open_key.create_subkey("command")?.0.set_raw_value(
        "",
        &create_expand_sz(
            "%SystemRoot%\\System32\\rundll32.exe \"%ProgramFiles%\\Windows Photo Viewer\\PhotoViewer.dll\", ImageView_Fullscreen %1",
        ),
    )?;
    open_key
        .create_subkey("DropTarget")?
        .0
        .set_value("", &CLSID_SHELL_IMAGE_PREVIEW)?;
    progid_shell_key.create_subkey("printto\\command")?.0.set_raw_value(
        "name",
        &create_expand_sz(
            "%SystemRoot%\\System32\\rundll32.exe \"%SystemRoot%\\System32\\shimgvw.dll\", ImageView_PrintTo /pt \"%1\" \"%2\" \"%3\" \"%4\"",
        ),
    )?;

    // Integration with Windows Thumbnail Cache — point at the stock
    // CLSID_PhotoThumbnailProvider, which dispatches through WIC and
    // therefore through our decoder.
    let (system_shell_ex, _) = system_ext_key.create_subkey("ShellEx")?;
    system_shell_ex
        .create_subkey(guid_to_string(
            &windows::Win32::UI::Shell::IThumbnailProvider::IID,
        ))?
        .0
        .set_value("", &CLSID_PHOTO_THUMBNAIL_PROVIDER)?;

    Ok(())
}

fn delete_default_if_same(subkey_path: &str, value: &str) -> std::io::Result<()> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    if let Ok(subkey) = hkcr.open_subkey_with_flags(subkey_path, KEY_READ | KEY_WRITE) {
        let rv: Result<String, _> = subkey.get_value("");
        if let Ok(val) = rv
            && val == value
        {
            subkey.delete_value("")?;
        }
    }
    Ok(())
}

fn unregister_provider() -> std::io::Result<()> {
    delete_default_if_same(
        &format!(
            "SystemFileAssociations\\{}\\ShellEx\\{{{:?}}}",
            EXT,
            windows::Win32::UI::Shell::IThumbnailProvider::IID
        ),
        CLSID_PHOTO_THUMBNAIL_PROVIDER,
    )?;

    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    hkcr.delete_subkey_all(format!("{}\\OpenWithProgids\\{}", EXT, PROGID))
        .ok();
    hkcr.delete_subkey_all(PROGID).ok();

    Ok(())
}

pub fn register(module_path: &str) -> std::io::Result<()> {
    register_clsid(module_path)?;
    register_provider()?;
    Ok(())
}

pub fn unregister() -> std::io::Result<()> {
    unregister_clsid();
    unregister_provider()?;
    Ok(())
}
