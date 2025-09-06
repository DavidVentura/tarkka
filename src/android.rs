use crate::reader::DictionaryReader;
use crate::{WordWithTaggedEntries, WordTag};
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
        Ok(Some(word_with_entries)) => unsafe {
            create_word_with_tagged_entries_jobject(&mut env, &word_with_entries)
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

unsafe fn create_word_with_tagged_entries_jobject(env: &mut JNIEnv, word: &WordWithTaggedEntries) -> jobject {
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

    // Create WordWithTaggedEntries object
    match env.new_object(
        "dev/davidv/translator/WordWithTaggedEntries",
        "(Ljava/lang/String;ILjava/util/List;)V",
        &[
            (&word_string).into(),
            tag_value.into(),
            (&entries_list).into(),
        ],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn create_word_entry_complete_jobject(env: &mut JNIEnv, entry: &crate::WordEntryComplete) -> jobject {
    let word_string = env.new_string(&entry.word).unwrap();
    
    // Create senses list
    let senses_list = match env.new_object("java/util/ArrayList", "()V", &[]) {
        Ok(list) => list,
        Err(_) => return std::ptr::null_mut(),
    };

    for sense in &entry.senses {
        let sense_obj = create_sense_jobject(env, sense);
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

    // Create hyphenations list
    let hyphenations_list = if let Some(hyphenations) = &entry.hyphenations {
        let list = match env.new_object("java/util/ArrayList", "()V", &[]) {
            Ok(list) => list,
            Err(_) => return std::ptr::null_mut(),
        };
        
        for hyphenation in hyphenations {
            let parts_list = create_string_list(env, Some(&hyphenation.parts));
            let _ = env.call_method(
                &list,
                "add",
                "(Ljava/lang/Object;)Z",
                &[(&unsafe { JObject::from_raw(parts_list) }).into()],
            );
        }
        list.into_raw()
    } else {
        std::ptr::null_mut()
    };

    // Create sounds list  
    let sounds_list = if let Some(sounds) = &entry.sounds {
        let list = match env.new_object("java/util/ArrayList", "()V", &[]) {
            Ok(list) => list,
            Err(_) => return std::ptr::null_mut(),
        };
        
        for sound in sounds {
            let ipa_string = if let Some(ipa) = &sound.ipa {
                env.new_string(ipa).unwrap()
            } else {
                env.new_string("").unwrap()
            };
            
            let sound_obj = match env.new_object(
                "dev/davidv/translator/Sound",
                "(Ljava/lang/String;)V",
                &[(&ipa_string).into()],
            ) {
                Ok(obj) => obj,
                Err(_) => return std::ptr::null_mut(),
            };
            
            let _ = env.call_method(
                &list,
                "add", 
                "(Ljava/lang/Object;)Z",
                &[(&sound_obj).into()],
            );
        }
        list.into_raw()
    } else {
        std::ptr::null_mut()
    };

    match env.new_object(
        "dev/davidv/translator/WordEntryComplete",
        "(Ljava/lang/String;Ljava/util/List;Ljava/util/List;Ljava/util/List;)V",
        &[
            (&word_string).into(),
            (&senses_list).into(),
            (&unsafe { JObject::from_raw(hyphenations_list) }).into(),
            (&unsafe { JObject::from_raw(sounds_list) }).into(),
        ],
    ) {
        Ok(obj) => obj.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

unsafe fn create_sense_jobject(env: &mut JNIEnv, sense: &crate::Sense) -> jobject {
    let pos_string = env.new_string(&sense.pos).unwrap();
    
    let glosses_list = if let Some(glosses) = &sense.glosses {
        create_string_list(env, Some(glosses))
    } else {
        std::ptr::null_mut()
    };

    let form_of_list = if let Some(form_of) = &sense.form_of {
        let list = match env.new_object("java/util/ArrayList", "()V", &[]) {
            Ok(list) => list,
            Err(_) => return std::ptr::null_mut(),
        };
        
        for form in form_of {
            let form_word = env.new_string(&form.word).unwrap();
            let _ = env.call_method(
                &list,
                "add",
                "(Ljava/lang/Object;)Z",
                &[(&form_word).into()],
            );
        }
        list.into_raw()
    } else {
        std::ptr::null_mut()
    };

    match env.new_object(
        "dev/davidv/translator/Sense",
        "(Ljava/lang/String;Ljava/util/List;Ljava/util/List;)V",
        &[
            (&pos_string).into(),
            (&unsafe { JObject::from_raw(glosses_list) }).into(),
            (&unsafe { JObject::from_raw(form_of_list) }).into(),
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