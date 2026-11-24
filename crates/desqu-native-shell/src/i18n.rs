//! Words in the language of the user. The program is written in English: a text is looked up by its English
//! wording and comes back in the chosen language, or unchanged when there is no translation. That way a missing
//! translation is only ever English, never a blank or a key.
//!
//! Swedish and Polish so far. The language is the one of Windows unless the user picked another.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Sv,
    Pl,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Sv, Lang::Pl];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Sv => "sv",
            Lang::Pl => "pl",
        }
    }

    /// The language's own name.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Sv => "Svenska",
            Lang::Pl => "Polski",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }

    /// The language for a Windows locale name such as `sv-SE` or `pl-PL` (English for any other).
    pub fn from_locale(name: &str) -> Lang {
        let primary = name.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        Lang::from_code(&primary).unwrap_or(Lang::En)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Sv,
        2 => Lang::Pl,
        _ => Lang::En,
    }
}

/// The language Windows is set to for this user.
#[cfg(windows)]
pub fn system_language() -> Lang {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85];
    // SAFETY: the buffer is local and its length is given.
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n <= 1 {
        return Lang::En;
    }
    Lang::from_locale(&String::from_utf16_lossy(&buf[..n as usize - 1]))
}

#[cfg(not(windows))]
pub fn system_language() -> Lang {
    Lang::En
}

/// `english` in the current language.
pub fn t(english: &'static str) -> &'static str {
    translate(current(), english)
}

pub fn translate(lang: Lang, english: &'static str) -> &'static str {
    let table = match lang {
        Lang::En => return english,
        Lang::Sv => SV,
        Lang::Pl => PL,
    };
    table.iter().find(|(en, _)| *en == english).map_or(english, |(_, local)| local)
}

/// Whether `english` has a translation into `lang` (English always does).
pub fn is_translated(lang: Lang, english: &str) -> bool {
    match lang {
        Lang::En => true,
        Lang::Sv => SV.iter().any(|(en, _)| *en == english),
        Lang::Pl => PL.iter().any(|(en, _)| *en == english),
    }
}

const SV: &[(&str, &str)] = &[
    ("Home", "Hem"),
    ("Settings", "Inställningar"),
    ("APPS", "APPAR"),
    ("Your apps", "Dina appar"),
    ("Turn on what you need. Each app runs on its own, and Desqu keeps them together.", "Slå på det du behöver. Varje app körs för sig, och Desqu håller ihop dem."),
    ("Start all apps with system", "Starta alla appar med Windows"),
    ("Start with system", "Starta med Windows"),
    ("Show its log", "Visa loggen"),
    ("Running", "Körs"),
    ("On", "På"),
    ("Off", "Av"),
    ("Running on its own", "Körs på egen hand"),
    ("Stopped unexpectedly", "Stannade oväntat"),
    ("Not installed", "Inte installerad"),
    ("Hotkeys", "Kortkommandon"),
    ("Command palette", "Kommandopalett"),
    ("Back up your settings", "Säkerhetskopiera inställningarna"),
    ("Back up now", "Säkerhetskopiera nu"),
    ("Open the backup folder", "Öppna mappen med kopior"),
    ("Restore", "Återställ"),
    ("Report a problem", "Rapportera ett problem"),
    ("Save a report", "Spara en rapport"),
    ("Updates", "Uppdateringar"),
    ("Check now", "Sök nu"),
    ("Download", "Hämta"),
    ("About", "Om"),
    ("Quit Desqu and its apps", "Avsluta Desqu och apparna"),
    ("Open the logs folder", "Öppna loggmappen"),
    ("Language", "Språk"),
    ("Automatic", "Automatiskt"),
    ("Desktops", "Skrivbord"),
    ("Next", "Nästa"),
    ("Done", "Klar"),
    ("Skip the tour", "Hoppa över rundturen"),
    ("Welcome to Desqu", "Välkommen till Desqu"),
    ("Desqu keeps a few small tools together: it starts them, stops them and holds their settings. Turn on only what you want; each tool runs on its own.", "Desqu samlar några små verktyg: det startar och stoppar dem och håller deras inställningar. Slå bara på det du vill ha; varje verktyg körs för sig."),
    ("Boxes on your desktop for icons. Drag a box on the empty desktop to make one, drop icons in, and the desktop stays tidy. A vial can also show a folder, your recent files or notes.", "Lådor på skrivbordet för ikoner. Dra en ruta på det tomma skrivbordet för att göra en, släpp ikoner i den så blir skrivbordet snyggt. En vial kan också visa en mapp, dina senaste filer eller anteckningar."),
    ("Gives each virtual desktop its own desktop icons. Switch desktops with Win+Tab and the icons change with them.", "Ger varje virtuellt skrivbord sina egna ikoner. Byt skrivbord med Win+Tab så byter ikonerna med."),
    ("Keeps fullscreen apps inside a part of your screen that you choose, so everything else stays in view. Pick the zone and the apps on its page.", "Håller helskärmsappar inom en del av skärmen som du väljer, så att allt annat syns. Välj zon och appar på dess sida."),
    ("Fences-style containers on your desktop: drag a box or right-click the desktop to create one, drag icons in and out, optional auto-sort rules.", "Behållare på skrivbordet som i Fences: dra en ruta eller högerklicka på skrivbordet för att skapa en, dra ikoner in och ut, valfria regler för automatisk sortering."),
    ("Tie desktop shortcuts, files and folders to a virtual desktop so each desktop shows only its own.", "Knyt genvägar, filer och mappar till ett virtuellt skrivbord så att varje skrivbord bara visar sina egna."),
    ("Keep fullscreen windows inside a zone of your monitor instead of covering all of it.", "Håll helskärmsfönster inom en zon på skärmen i stället för att täcka hela."),
    ("Zone and apps", "Zon och appar"),
    ("Groups, rules and browser", "Grupper, regler och webbläsare"),
];

const PL: &[(&str, &str)] = &[
    ("Home", "Start"),
    ("Settings", "Ustawienia"),
    ("APPS", "APLIKACJE"),
    ("Your apps", "Twoje aplikacje"),
    ("Turn on what you need. Each app runs on its own, and Desqu keeps them together.", "Włącz to, czego potrzebujesz. Każda aplikacja działa osobno, a Desqu trzyma je razem."),
    ("Start all apps with system", "Uruchamiaj wszystkie aplikacje z systemem"),
    ("Start with system", "Uruchamiaj z systemem"),
    ("Show its log", "Pokaż dziennik"),
    ("Running", "Działa"),
    ("On", "Wł."),
    ("Off", "Wył."),
    ("Running on its own", "Działa samodzielnie"),
    ("Stopped unexpectedly", "Zatrzymała się niespodziewanie"),
    ("Not installed", "Nie zainstalowano"),
    ("Hotkeys", "Skróty klawiszowe"),
    ("Command palette", "Paleta poleceń"),
    ("Back up your settings", "Kopia zapasowa ustawień"),
    ("Back up now", "Utwórz kopię teraz"),
    ("Open the backup folder", "Otwórz folder kopii"),
    ("Restore", "Przywróć"),
    ("Report a problem", "Zgłoś problem"),
    ("Save a report", "Zapisz raport"),
    ("Updates", "Aktualizacje"),
    ("Check now", "Sprawdź teraz"),
    ("Download", "Pobierz"),
    ("About", "Informacje"),
    ("Quit Desqu and its apps", "Zakończ Desqu i aplikacje"),
    ("Open the logs folder", "Otwórz folder dzienników"),
    ("Language", "Język"),
    ("Automatic", "Automatycznie"),
    ("Desktops", "Pulpity"),
    ("Next", "Dalej"),
    ("Done", "Gotowe"),
    ("Skip the tour", "Pomiń wprowadzenie"),
    ("Welcome to Desqu", "Witamy w Desqu"),
    ("Desqu keeps a few small tools together: it starts them, stops them and holds their settings. Turn on only what you want; each tool runs on its own.", "Desqu łączy kilka małych narzędzi: uruchamia je, zatrzymuje i przechowuje ich ustawienia. Włącz tylko to, co chcesz; każde narzędzie działa osobno."),
    ("Boxes on your desktop for icons. Drag a box on the empty desktop to make one, drop icons in, and the desktop stays tidy. A vial can also show a folder, your recent files or notes.", "Pudełka na ikony na pulpicie. Przeciągnij ramkę na pustym pulpicie, aby je utworzyć, wrzuć do niego ikony, a pulpit będzie uporządkowany. Pojemnik może też pokazywać folder, ostatnie pliki lub notatki."),
    ("Gives each virtual desktop its own desktop icons. Switch desktops with Win+Tab and the icons change with them.", "Daje każdemu pulpitowi wirtualnemu własne ikony. Przełączaj pulpity klawiszami Win+Tab, a ikony zmienią się razem z nimi."),
    ("Keeps fullscreen apps inside a part of your screen that you choose, so everything else stays in view. Pick the zone and the apps on its page.", "Utrzymuje aplikacje pełnoekranowe w wybranej części ekranu, aby wszystko inne pozostało widoczne. Wybierz strefę i aplikacje na jej stronie."),
    ("Fences-style containers on your desktop: drag a box or right-click the desktop to create one, drag icons in and out, optional auto-sort rules.", "Pojemniki na pulpicie w stylu Fences: przeciągnij ramkę lub kliknij pulpit prawym przyciskiem, aby je utworzyć, przeciągaj ikony tam i z powrotem, opcjonalne reguły automatycznego sortowania."),
    ("Tie desktop shortcuts, files and folders to a virtual desktop so each desktop shows only its own.", "Przypisz skróty, pliki i foldery do pulpitu wirtualnego, aby każdy pulpit pokazywał tylko swoje."),
    ("Keep fullscreen windows inside a zone of your monitor instead of covering all of it.", "Utrzymuj okna pełnoekranowe w strefie monitora zamiast zasłaniać go całego."),
    ("Zone and apps", "Strefa i aplikacje"),
    ("Groups, rules and browser", "Grupy, reguły i przeglądarka"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locale_name_gives_its_language_and_anything_else_is_english() {
        assert_eq!(Lang::from_locale("sv-SE"), Lang::Sv);
        assert_eq!(Lang::from_locale("pl-PL"), Lang::Pl);
        assert_eq!(Lang::from_locale("en-US"), Lang::En);
        assert_eq!(Lang::from_locale("de-DE"), Lang::En);
        assert_eq!(Lang::from_locale(""), Lang::En);
        assert_eq!(Lang::from_code("sv"), Some(Lang::Sv));
        assert_eq!(Lang::from_code("xx"), None);
    }

    #[test]
    fn a_text_comes_back_translated_or_as_it_was() {
        assert_eq!(translate(Lang::Sv, "Home"), "Hem");
        assert_eq!(translate(Lang::Pl, "Home"), "Start");
        assert_eq!(translate(Lang::En, "Home"), "Home");
        assert_eq!(translate(Lang::Sv, "A text nobody translated"), "A text nobody translated");
    }

    #[test]
    fn the_tables_have_no_empty_texts_and_no_text_twice() {
        for (name, table) in [("sv", SV), ("pl", PL)] {
            let mut seen = std::collections::HashSet::new();
            for (en, local) in table {
                assert!(!en.is_empty() && !local.is_empty(), "{name}: {en}");
                assert!(seen.insert(*en), "{name}: {en} is in the table twice");
            }
        }
    }

    #[test]
    fn the_current_language_is_what_was_set() {
        set(Lang::Sv);
        assert_eq!(t("Done"), "Klar");
        set(Lang::En);
        assert_eq!(t("Done"), "Done");
    }
}
