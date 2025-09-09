use crate::WordWithTaggedEntries;
use crate::reader::DictionaryReader;
use std::ffi::CString;
use std::fs::File;

extern crate jni;
use self::jni::JNIEnv;
use self::jni::objects::{JClass, JObject, JString};
use self::jni::sys::{jlong, jobject};

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: i32, tag: *const i8, text: *const i8) -> i32;
}

const ANDROID_LOG_DEBUG: i32 = 3;

macro_rules! android_log {
    ($msg:expr) => {
        unsafe {
            let tag = CString::new("TarkkaNative").unwrap();
            let message = CString::new($msg).unwrap();
            __android_log_write(ANDROID_LOG_DEBUG, tag.as_ptr(), message.as_ptr());
        }
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_davidv_translator_TarkkaBinding_nativeOpen(
    mut env: JNIEnv,
    _: JClass,
    java_path: JString,
) -> jlong {
    let path: String = match env.get_string(&java_path) {
        Ok(path) => path.into(),
        Err(_) => return 0,
    };

    match File::open(&path) {
        Ok(file) => match DictionaryReader::open(file) {
            Ok(reader) => {
                let boxed_reader = Box::new(reader);
                Box::into_raw(boxed_reader) as jlong
            }
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_davidv_translator_TarkkaBinding_nativeLookup(
    mut env: JNIEnv,
    _: JClass,
    reader_ptr: jlong,
    java_word: JString,
) -> jobject {
    android_log!("nativeLookup: Started");

    if reader_ptr == 0 {
        android_log!("nativeLookup: reader_ptr is 0, returning null");
        return std::ptr::null_mut();
    }

    let word: String = match env.get_string(&java_word) {
        Ok(word) => {
            let w: String = word.into();
            android_log!(format!("nativeLookup: Looking up word: {}", w));
            w
        }
        Err(_) => {
            android_log!("nativeLookup: Failed to get string from java_word");
            return std::ptr::null_mut();
        }
    };

    android_log!("nativeLookup: Getting reader from pointer");
    let reader = unsafe { &mut *(reader_ptr as *mut DictionaryReader<File>) };

    android_log!("nativeLookup: Calling reader.lookup");
    match reader.lookup(&word) {
        Ok(Some(word_with_entries)) => {
            android_log!("nativeLookup: Found word, creating Java object");
            unsafe {
                let result = create_word_with_tagged_entries_jobject(&mut env, &word_with_entries);
                if result.is_null() {
                    android_log!(
                        "nativeLookup: create_word_with_tagged_entries_jobject returned null"
                    );
                } else {
                    android_log!("nativeLookup: Successfully created Java object");
                }
                result
            }
        }
        Ok(None) => {
            android_log!("nativeLookup: Word not found in dictionary");
            std::ptr::null_mut()
        }
        Err(e) => {
            android_log!(format!("nativeLookup: Lookup error: {:?}", e));
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_davidv_translator_TarkkaBinding_nativeClose(
    _env: JNIEnv,
    _: JClass,
    reader_ptr: jlong,
) {
    if reader_ptr != 0 {
        let _ = unsafe { Box::from_raw(reader_ptr as *mut DictionaryReader<File>) };
    }
}

unsafe fn create_word_with_tagged_entries_jobject(
    env: &mut JNIEnv,
    word: &WordWithTaggedEntries,
) -> jobject {
    // Create ArrayList for entries
    let entries_list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    for entry in &word.entries {
        let entry_obj = create_word_entry_complete_jobject(env, entry);
        if entry_obj.is_null() {
            return std::ptr::null_mut();
        }

        let _ = env.call_method(
            &entries_list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[(&unsafe { JObject::from_raw(entry_obj) }).into()],
        );
    }

    let word_string = env.new_string(&word.word).unwrap();
    let tag_value = word.tag as i32;

    let sounds_param = if let Some(ref sounds) = word.sounds {
        env.new_string(sounds).ok()
    } else {
        None
    };

    // Create hyphenations list
    let hyphenations_list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    for hyphenation in &word.hyphenations {
        let hyph_string = env.new_string(hyphenation).unwrap();
        let _ = env.call_method(
            &hyphenations_list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[(&hyph_string).into()],
        );
    }

    let null_obj = JObject::null();
    let sounds_jvalue = if let Some(ref jstring) = sounds_param {
        jstring.into()
    } else {
        (&null_obj).into()
    };

    match env.new_object(
        "dev/davidv/translator/WordWithTaggedEntries",
        "(Ljava/lang/String;ILjava/util/List;Ljava/lang/String;Ljava/util/List;)V",
        &[
            (&word_string).into(),
            tag_value.into(),
            (&entries_list).into(),
            sounds_jvalue,
            (&hyphenations_list).into(),
        ],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn create_word_entry_complete_jobject(
    env: &mut JNIEnv,
    entry: &crate::WordEntryComplete,
) -> jobject {
    // Create senses list
    let senses_list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    for sense in &entry.senses {
        let sense_obj = unsafe { create_sense_jobject(env, sense) };
        if sense_obj.is_null() {
            return std::ptr::null_mut();
        }

        let _ = env.call_method(
            &senses_list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[(&unsafe { JObject::from_raw(sense_obj) }).into()],
        );
    }

    match env.new_object(
        "dev/davidv/translator/WordEntryComplete",
        "(Ljava/util/List;)V",
        &[(&senses_list).into()],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn create_sense_jobject(env: &mut JNIEnv, sense: &crate::Sense) -> jobject {
    let pos_string = env.new_string(&sense.pos).unwrap();

    let list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };
    for gloss in &sense.glosses {
        let gloss_obj = unsafe { create_gloss_jobject(env, gloss) };
        if gloss_obj.is_null() {
            return std::ptr::null_mut();
        }

        let _ = env.call_method(
            &list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[(&unsafe { JObject::from_raw(gloss_obj) }).into()],
        );
    }
    let glosses_list = list.into_raw();

    match env.new_object(
        "dev/davidv/translator/Sense",
        "(Ljava/lang/String;Ljava/util/List;)V",
        &[
            (&pos_string).into(),
            (&unsafe { JObject::from_raw(glosses_list) }).into(),
        ],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn create_gloss_jobject(env: &mut JNIEnv, gloss: &crate::Gloss) -> jobject {
    let gloss_lines_list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    for line in &gloss.gloss_lines {
        let line_string = env.new_string(line).unwrap();
        let _ = env.call_method(
            &gloss_lines_list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[(&line_string).into()],
        );
    }

    match env.new_object(
        "dev/davidv/translator/Gloss",
        "(Ljava/util/List;)V",
        &[(&gloss_lines_list).into()],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn create_string_list(env: &mut JNIEnv, strings: Option<&Vec<String>>) -> jobject {
    let list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    if let Some(vec) = strings {
        for s in vec {
            let jstring = match env.new_string(s) {
                Ok(jstring) => jstring,
                Err(_) => continue,
            };
            let _ = env.call_method(&list, "add", "(Ljava/lang/Object;)Z", &[(&jstring).into()]);
        }
    }

    list.into_raw()
}
