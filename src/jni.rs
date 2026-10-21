use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use std::path::Path;

fn make_jstring(env: &mut JNIEnv, s: String) -> jstring {
    match env.new_string(s) {
        Ok(v) => v.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn get_rust_string(env: &mut JNIEnv, input: JString) -> Result<String, String> {
    env.get_string(&input)
        .map(|s| s.into())
        .map_err(|e| e.to_string())
}

fn error_json(msg: String) -> String {
    let escaped = msg.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{{\"error\":\"{}\"}}", escaped)
}

#[no_mangle]
pub extern "system" fn Java_id_managerkebiri_dolin_DolinNative_archiveInfoReportJson(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let path_str = match get_rust_string(&mut env, path) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let out = match crate::archive_info_report_json(Path::new(&path_str)) {
        Ok(json) => json,
        Err(e) => error_json(e.to_string()),
    };

    make_jstring(&mut env, out)
}

#[no_mangle]
pub extern "system" fn Java_id_managerkebiri_dolin_DolinNative_listArchiveReportJson(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let path_str = match get_rust_string(&mut env, path) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let out = match crate::list_archive_report_json(Path::new(&path_str)) {
        Ok(json) => json,
        Err(e) => error_json(e.to_string()),
    };

    make_jstring(&mut env, out)
}

#[no_mangle]
pub extern "system" fn Java_id_managerkebiri_dolin_DolinNative_validateArchiveReportJson(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) -> jstring {
    let path_str = match get_rust_string(&mut env, path) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let out = match crate::validate_archive_report_json(Path::new(&path_str)) {
        Ok(json) => json,
        Err(e) => error_json(e.to_string()),
    };

    make_jstring(&mut env, out)
}


#[no_mangle]
pub extern "system" fn Java_id_managerkebiri_dolin_DolinNative_extractArchiveEntryReportJson(
    mut env: JNIEnv,
    _class: JClass,
    archive_path: JString,
    entry_path: JString,
    output_file: JString,
) -> jstring {
    let archive_path_str = match get_rust_string(&mut env, archive_path) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let entry_path_str = match get_rust_string(&mut env, entry_path) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let output_file_str = match get_rust_string(&mut env, output_file) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let out = match crate::extract_archive_entry_report_json(
        Path::new(&archive_path_str),
        &entry_path_str,
        Path::new(&output_file_str),
    ) {
        Ok(json) => json,
        Err(e) => error_json(e.to_string()),
    };

    make_jstring(&mut env, out)
}


#[no_mangle]
pub extern "system" fn Java_id_managerkebiri_dolin_DolinNative_extractArchiveReportJson(
    mut env: JNIEnv,
    _class: JClass,
    archive_path: JString,
    output_dir: JString,
) -> jstring {
    let archive_path_str = match get_rust_string(&mut env, archive_path) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let output_dir_str = match get_rust_string(&mut env, output_dir) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let out = match crate::extract_archive_report_json(
        Path::new(&archive_path_str),
        Path::new(&output_dir_str),
    ) {
        Ok(json) => json,
        Err(e) => error_json(e.to_string()),
    };

    make_jstring(&mut env, out)
}


#[no_mangle]
pub extern "system" fn Java_id_managerkebiri_dolin_DolinNative_packPathReportJson(
    mut env: JNIEnv,
    _class: JClass,
    input_path: JString,
    output_archive: JString,
) -> jstring {
    let input_path_str = match get_rust_string(&mut env, input_path) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let output_archive_str = match get_rust_string(&mut env, output_archive) {
        Ok(v) => v,
        Err(e) => return make_jstring(&mut env, error_json(e)),
    };

    let out = match crate::pack_path_report_json(
        Path::new(&input_path_str),
        Path::new(&output_archive_str),
    ) {
        Ok(json) => json,
        Err(e) => error_json(e.to_string()),
    };

    make_jstring(&mut env, out)
}

