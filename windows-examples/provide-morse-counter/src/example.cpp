PERF_OBJECT_TYPE type = {0};
PERF_COUNTER_DEFINITION counter1 = {0};
PERF_COUNTER_DEFINITION counter2 = {0};
PERF_COUNTER_BLOCK bl = {0};

const WCHAR *hw = L"Hello, World!";

// Округляет v вверх до кратности 8, напишите её сами ;)
DWORD UpTo8(DWORD v);

//// Вызывается при загрузке, инициализирует глобальные структуры.
extern "C" __declspec(dllexport) DWORD Open(LPWSTR lpDeviceNames) {
    DWORD name;
    DWORD help;

    // Читает реестр, получает значения «First Counter» и «First Help»
    DWORD res = Registry::GetFirst(&name, &help);
    if (res != ERROR_SUCCESS) {
        // Не судьба. Вероятно, библиотека некорректно зарегистрирована.return res;
    }

    // Инициализируем описание объекта. Размер объекта вычисляется в конце.
    type.ObjectNameTitleIndex = name + TYPE_OFFSET;
    type.ObjectHelpTitleIndex = help + TYPE_OFFSET;
    type.NumCounters = 2;                   // Два счётчика
    type.NumInstances = PERF_NO_INSTANCES;   // Никаких экземпляров
    type.HeaderLength = sizeof(type);

    // Инициализируем описание первого счётчика
    counter1.CounterNameTitleIndex = name + COUNTER1_OFFSET;
    counter1.CounterHelpTitleIndex = help + COUNTER1_OFFSET;
    counter1.CounterSize = (lstrlenW(hw) + 1) * sizeof(WCHAR);
    counter1.CounterType = PERF_SIZE_VARIABLE_LEN | PERF_TYPE_TEXT
                           | PERF_TEXT_UNICODE;
    // До него должна влезть структура PERF_COUNTER_BLOCK
    counter1.CounterOffset = sizeof(bl);
    counter1.ByteLength = sizeof(counter1);

    // Инициализируем описание второго счётчика
    counter2.CounterNameTitleIndex = name + COUNTER2_OFFSET;
    counter2.CounterHelpTitleIndex = help + COUNTER2_OFFSET;
    counter2.CounterSize = sizeof(DWORD);
    counter2.CounterType = PERF_SIZE_DWORD | PERF_TYPE_NUMBER;
    // Идёт сразу после первого
    counter2.CounterOffset = counter1.CounterOffset + counter1.CounterSize;
    counter2.ByteLength = sizeof(counter2);

    // размер данных – смещение последнего счётчика плюс его длина
    bl.ByteLength = counter2.CounterOffset + counter2.CounterSize;

    type.DefinitionLength = type.HeaderLength +
                            counter1.ByteLength +
                            counter2.ByteLength;

    // Размер объекта должен быть кратен 8 байтам, иначе в EventLog добавится// сообщение, рекомендующее обратиться к производителю за новой версией dll.
    type.TotalByteLength = UpTo8(type.DefinitionLength + bl.ByteLength);

    return ERROR_SUCCESS;
}

//// Вызывается при сборе данных. Не анализирует строку
// запроса, просто копирует данные в буфер.
extern "C"__declspec(dllexport)

DWORD Collect(
        LPWSTR lpwszValue,
        LPVOID *lppData,
        LPDWORD lpcbBytes,
        LPDWORD lpcObjectTypes
) {
    if (*lpcbBytes < type.TotalByteLength) {
// Не влезаем
        *lpcbBytes = 0;
        *lpcObjectTypes = 0;
        return ERROR_MORE_DATA;
    }

    char *temp = (char *) (*lppData);

// Копируем описание объекта
    memcpy(temp, &type, sizeof(type));
    temp += type.HeaderLength;

// Копируем описание первого счётчика
    memcpy(temp, &counter1, sizeof(counter1));
    temp += counter1.ByteLength;

// Копируем описание второго счётчика
    memcpy(temp, &counter2, sizeof(counter2));
    temp += counter2.ByteLength;

// Копируем заголовок блока данных
    memcpy(temp, &bl, sizeof(bl));

// Копируем данные первого счётчика
    memcpy(temp + counter1.CounterOffset, hw, (lstrlenW(hw) + 1) * sizeof(WCHAR));

    DWORD v = rand() % 10;

// Копируем данные второго счётчика
    memcpy(temp + counter2.CounterOffset, &v, sizeof(DWORD));

// Устанавливаем выходные параметры
    *lppData = (char *) (*lppData) + type.TotalByteLength;
    *lpcbBytes = type.TotalByteLength;
    *lpcObjectTypes = 1;

    return ERROR_SUCCESS;
}

extern "C" __declspec(dllexport) DWORD Close() {
    return ERROR_SUCCESS;
}