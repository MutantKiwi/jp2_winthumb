use windows::core::GUID;

// Freshly generated vendor CLSID — distinct from the JXL version so the two
// handlers can coexist on the same machine.
pub const JP2WINTHUMB_VENDOR_CLSID: GUID =
    GUID::from_u128(0x6b3e1c58_9f72_4a85_8d4c_5e7a9b3d2f68);

pub fn guid_to_string(guid: &GUID) -> String {
    format!("{{{:?}}}", guid)
}
