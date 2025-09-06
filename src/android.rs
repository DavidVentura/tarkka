use crate::reader::DictionaryReader;
use crate::{AggregatedWord, Gloss, PosGlosses};
use std::fs::File;

extern crate jni;
use self::jni::objects::{JClass, JObject, JString};
use self::jni::sys::{jlong, jobject};
use self::jni::JNIEnv;

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
    if reader_ptr == 0 {
        return std::ptr::null_mut();
    }

    let word: String = match env.get_string(&java_word) {
        Ok(word) => word.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let reader = unsafe { &mut *(reader_ptr as *mut DictionaryReader<File>) };

    match reader.lookup(&word) {
        Ok(Some(aggregated_word)) => unsafe {
            create_aggregated_word_jobject(&mut env, &aggregated_word)
        },
        _ => std::ptr::null_mut(),
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

unsafe fn create_aggregated_word_jobject(env: &mut JNIEnv, word: &AggregatedWord) -> jobject {
    // Create ArrayList for pos_glosses
    let pos_glosses_list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    for pos_gloss in &word.pos_glosses {
        let pos_gloss_obj = create_pos_glosses_jobject(env, pos_gloss);
        if pos_gloss_obj.is_null() {
            return std::ptr::null_mut();
        }

        let _ = env.call_method(
            &pos_glosses_list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[(&unsafe { JObject::from_raw(pos_gloss_obj) }).into()],
        );
    }

    // Create string lists for optional fields
    let hyphenation_list = create_string_list(env, word.hyphenation.as_ref());
    let form_of_list = create_string_list(env, word.form_of.as_ref());
    let ipa_sound_list = create_string_list(env, word.ipa_sound.as_ref());

    // Create AggregatedWord object
    let word_string = env.new_string(&word.word).unwrap();

    match env.new_object(
        "dev/davidv/translator/AggregatedWord",
        "(Ljava/lang/String;Ljava/util/List;Ljava/util/List;Ljava/util/List;Ljava/util/List;)V",
        &[
            (&word_string).into(),
            (&pos_glosses_list).into(),
            (&unsafe { JObject::from_raw(hyphenation_list) }).into(),
            (&unsafe { JObject::from_raw(form_of_list) }).into(),
            (&unsafe { JObject::from_raw(ipa_sound_list) }).into(),
        ],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn create_pos_glosses_jobject(env: &mut JNIEnv, pos_gloss: &PosGlosses) -> jobject {
    let pos_string = env.new_string(&pos_gloss.pos).unwrap();

    let glosses_list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    for gloss in &pos_gloss.glosses {
        let gloss_obj = create_gloss_jobject(env, gloss);
        if gloss_obj.is_null() {
            return std::ptr::null_mut();
        }

        let _ = env.call_method(
            &glosses_list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[(&unsafe { JObject::from_raw(gloss_obj) }).into()],
        );
    }

    match env.new_object(
        "dev/davidv/translator/PosGlosses",
        "(Ljava/lang/String;Ljava/util/List;)V",
        &[(&pos_string).into(), (&glosses_list).into()],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn create_gloss_jobject(env: &mut JNIEnv, gloss: &Gloss) -> jobject {
    let gloss_string = env.new_string(&gloss.gloss).unwrap();
    let categories_list = create_string_list(env, Some(&gloss.new_categories));

    match env.new_object(
        "dev/davidv/translator/Gloss",
        "(ILjava/lang/String;Ljava/util/List;)V",
        &[
            (gloss.shared_prefix_count as i32).into(),
            (&gloss_string).into(),
            (&unsafe { JObject::from_raw(categories_list) }).into(),
        ],
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
            let _ = env.call_method(
                &list,
                "add",
                "(Ljava/lang/Object;)Z",
                &[(&jstring).into()],
            );
        }
    }

    list.into_raw()
}