extern crate windows_rust_test;

use windows_rust_test::win::perf::consume::*;
use windows_rust_test::win::uses::*;

fn main() {
    let lang_id = get_language_id().expect("GetLanguageId");
    let locale = UseLocale::LangId(lang_id);

    let table = match get_counters_info(None, locale) {
        Ok(table) => table,
        Err(e) => {
            println!("Error while building counters table: {}", e);
            return;
        }
    };

    for counter in table.map().values() {
        println!("[{}]: Name = {}", counter.name_index, counter.name_value);
        println!("[{}]: Help = {}", counter.help_index, counter.help_value);
        println!();
    }
}
