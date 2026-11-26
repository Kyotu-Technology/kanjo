import type en from './extras.en';

const pl: typeof en = {
  shortcuts: {
    title: 'Skróty klawiszowe',
    intro: 'Z Kanjo można korzystać bez myszy. Te klawisze działają na każdej stronie.',
    open: 'Pokaż skróty klawiszowe',
    palette: 'Otwórz menu poleceń, aby szukać, przejść do strony albo wykonać akcję',
    help: 'Pokaż tę listę',
    tab: 'Przejdź do następnego elementu; z Shift wróć do poprzedniego',
    activate: 'Naciśnij przycisk, otwórz link albo przełącz przełącznik',
    escape: 'Zamknij okno, menu albo objaśnienie',
    arrows: 'Przechodź między kartami i opcjami w grupie',
    palette_move: 'Przechodź między wynikami w menu poleceń',
    palette_open: 'Otwórz zaznaczony wynik',
    skip: 'Pierwsze naciśnięcie Tab na stronie pozwala przeskoczyć prosto do treści',
    keys_tab: 'Tab',
    keys_enter: 'Enter lub spacja',
    keys_escape: 'Esc',
    keys_arrows: 'Strzałki',
    keys_updown: 'Strzałki w górę i w dół',
    keys_return: 'Enter',
    general: 'Wszędzie',
    in_palette: 'W menu poleceń',
    hint: 'Naciśnij {key} w dowolnej chwili, aby zobaczyć tę listę.',
  },
  not_found: {
    code: '404',
    title: 'Tej strony nie ma w Kanjo',
    body: 'Adres {path} nie pasuje do żadnej strony. Może pochodzić ze starej zakładki albo z sesji lub projektu, które zostały usunięte.',
    home: 'Otwórz pulpit',
    search: 'Szukaj w Kanjo',
    try: 'Albo przejdź od razu do',
  },
  print: {
    header: 'Raport Kanjo, wydrukowano {date}. Liczby zapisane na tym komputerze.',
    footer: 'Kwoty mają oznaczoną wiarygodność: Oficjalne pochodzą z Claude Code, Szacunek wylicza Kanjo, Co najmniej to suma z plików sesji.',
  },
};

export default pl;
