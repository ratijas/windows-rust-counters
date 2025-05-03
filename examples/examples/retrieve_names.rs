use win_high::perf::consume::*;
use win_high::prelude::v2::*;
use win_low::um::winnt::MAKELANGID;

fn main() {
    println!("English locale:");
    do_local_counters(UseLocale::English, Some(3));

    println!("Default UI locale:");
    do_local_counters(UseLocale::UIDefault, None);

    println!("Current language ID as custom LANGID :");
    let lang_id = get_language_id().expect("GetLanguageId");
    do_local_counters(UseLocale::LangId(lang_id), Some(3));

    println!("Custom LANGID :");
    let lang_id = MAKELANGID(LANG_RUSSIAN as u16, 0);
    do_local_counters(UseLocale::LangId(lang_id), Some(3));
}

// 'local' in `do_local_counters` means local machine, as opposed to a remote one.
fn do_local_counters(locale: UseLocale, limit: Option<usize>) {
    let table = match get_counters_info(None, locale) {
        Ok(table) => table,
        Err(e) => {
            println!("Error while building counters table: {}", e);
            return;
        }
    };
    print_counters(&table, limit);
}

fn print_counters(table: &AllCounters, limit: Option<usize>) {
    for counter in table
        .map()
        .values()
        .take(limit.unwrap_or(table.map().len()))
    {
        println!("[{}]: Name = {}", counter.name_index, counter.name_value);
        println!("[{}]: Help = {}", counter.help_index, counter.help_value);
        println!();
    }

    if let Some(limit) = limit {
        if limit < table.map().len() {
            println!("...");
            println!();
        }
    }
}
