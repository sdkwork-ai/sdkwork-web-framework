//! Admin API pagination aligned with `sdkwork-specs/PAGINATION_SPEC.md` and `sdkwork-utils-rust`.

use sdkwork_utils_rust::{
    cursor_list_page_data, offset_list_page_data, validated_offset_list_params,
    OffsetListPageParams, SdkWorkPageData, SdkWorkResultCode, DEFAULT_LIST_PAGE_SIZE,
    MAX_LIST_PAGE_SIZE,
};

pub fn validated_offset_params(
    page: Option<i64>,
    page_size: Option<i64>,
    legacy_limit: Option<i64>,
) -> Result<OffsetListPageParams, SdkWorkResultCode> {
    let merged_page_size = page_size.or(legacy_limit);
    if merged_page_size == Some(0) {
        return Err(SdkWorkResultCode::InvalidParameter);
    }
    validated_offset_list_params(page, merged_page_size)
}

pub fn validated_keyset_page_size(
    page_size: Option<i32>,
    legacy_limit: Option<i32>,
) -> Result<u32, SdkWorkResultCode> {
    let raw = page_size.or(legacy_limit);
    match raw {
        None => Ok(DEFAULT_LIST_PAGE_SIZE as u32),
        Some(0) => Err(SdkWorkResultCode::InvalidParameter),
        Some(value) if value < 0 || value > MAX_LIST_PAGE_SIZE => {
            Err(SdkWorkResultCode::InvalidParameter)
        }
        Some(value) => Ok(value as u32),
    }
}

pub fn parse_keyset_id_cursor(cursor: Option<&str>) -> Result<Option<i64>, SdkWorkResultCode> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    cursor
        .parse::<i64>()
        .map(Some)
        .map_err(|_| SdkWorkResultCode::InvalidParameter)
}

pub fn offset_page<T>(
    items: Vec<T>,
    total_items: i64,
    params: OffsetListPageParams,
) -> SdkWorkPageData<T> {
    offset_list_page_data(items, total_items, params)
}

pub fn keyset_page<T>(
    items: Vec<T>,
    page_size: u32,
    next_cursor: Option<String>,
    has_more: bool,
) -> SdkWorkPageData<T> {
    cursor_list_page_data(items, page_size as usize, next_cursor, has_more)
}

pub use sdkwork_utils_rust::{SdkWorkCursorListQuery, SdkWorkPageSizeQuery};
