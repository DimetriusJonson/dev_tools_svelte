import { derived, get, writable } from 'svelte/store';

export type Locale = 'en' | 'ru';

type Dictionary = Record<string, string>;

const en: Dictionary = {
	'nav.xml': 'XML',
	'nav.json': 'JSON',
	'nav.url': 'URL',
	'nav.compare': 'Compare',
	'nav.share': 'Share',
	'nav.rest': 'Rest client',
	'common.copy': 'Copy to clipboard',
	'common.copied': 'Copied!',
	'common.download': 'Save result',
	'common.selectFile': 'Select file',
	'common.noFile': 'No file selected',
	'common.format': 'Format',
	'common.formatting': 'Formatting…',
	'common.escape': 'Escape',
	'common.error': 'Something went wrong',
	'formatter.source': 'Source content',
	'formatter.output': 'Result',
	'formatter.format': 'Format',
	'formatter.unescape': 'Unescape',
	'formatter.indent': '{n} spaces',
	'formatter.sourcePlaceholder': 'Paste your XML here…',
	'formatter.sourcePlaceholderJson': 'Paste your JSON here…',
	'formatter.outputPlaceholder': 'The formatted result will appear here',
	'formatter.invalid': 'Could not process the content. Please check the syntax.',
	'formatter.saved': 'Saved to {path}',
	'formatter.fileDone': 'Done. Output written to {path}',
	'formatter.fileError': 'File processing failed: {message}',
	'url.encode': 'Encode',
	'url.decode': 'Decode',
	'url.sourcePlaceholder': 'Paste a URL or a query string…',
	'url.outputPlaceholder': 'The result will appear here',
	'compare.source': 'Source data',
	'compare.result': 'Result',
	'compare.compare': 'Compare',
	'compare.swap': 'Swap sides',
	'compare.identical': 'The two sides are identical',
	'compare.summary': '{added} added · {removed} removed · {changed} changed',
	'compare.placeholderLeft': 'Paste the original text…',
	'compare.placeholderRight': 'Paste the text to compare…',
	'share.title': 'Share',
	'share.description':
		'Share files and snippets over your local network. Uploads stay in the app database; links open the built-in viewer where the app is installed.',
	'share.localOnly': 'Local network',
	'share.shareResult': 'Share result',
	'share.server': 'Share server',
	'share.serverRunning': 'Server running',
	'share.serverError': 'Server unavailable',
	'share.serverUnavailable': 'The share server is not running. Retry the start or restart the app.',
	'share.retry': 'Retry server',
	'share.retrying': 'Restarting server…',
	'share.draftHint': 'The result is read-only. Change it in the XML or JSON tool if needed.',
	'share.serverHint': 'Choose the address other devices on this network can reach.',
	'share.loopback': 'this device only',
	'share.loopbackHint': 'No LAN interface was found. Other devices will not be able to connect.',
	'share.fileMode': 'File',
	'share.contentMode': 'Content',
	'share.dropHint': 'Drop a file here or select one',
	'share.dropActive': 'Drop the file to share',
	'share.limit': 'Maximum {size}',
	'share.uploadFile': 'Upload file',
	'share.uploading': 'Uploading…',
	'share.uploadContent': 'Upload content',
	'share.filename': 'File name',
	'share.language': 'Format',
	'share.plainText': 'Text',
	'share.contentPlaceholder': 'Paste the content to share…',
	'share.ready': 'Link ready',
	'share.link': 'Share link',
	'share.copyLink': 'Copy link',
	'share.linkHint': 'The link opens the desktop viewer. File size: {size}.',
	'share.loading': 'Starting share server…',
	'share.serverRequired': 'Select a share server first.',
	'share.emptyContent': 'Add some content before uploading.',
	'share.tooLarge': 'Content is too large. Maximum size is {size}.',
	'share.desktopOnly': 'Sharing works in the desktop app so files stay on your network.',
	'viewer.back': 'Back to Share',
	'viewer.loading': 'Loading shared file…',
	'viewer.retry': 'Retry',
	'viewer.binary': 'Binary file',
	'viewer.binaryHint': 'This file cannot be previewed. Download it to open locally.',
	'viewer.filename': 'File name',
	'viewer.type': 'Type',
	'viewer.size': 'Size',
	'viewer.created': 'Shared',
	'viewer.download': 'Download',
	'viewer.downloading': 'Downloading…',
	'viewer.downloaded': 'Saved to {path}',
	'viewer.content': 'Shared content',
	'viewer.desktopOnly': 'The shared file viewer is available in the desktop app only.',
	'viewer.invalidLink': 'This share link is invalid or incomplete.',
	'rest.title': 'Rest client',
	'rest.description':
		'Compose HTTP requests, inspect responses and keep a history of your API calls without leaving the app. This section is on its way.',
	'rest.badge': 'Coming soon',
	'theme.system': 'Theme: system',
	'theme.dark': 'Theme: dark',
	'theme.light': 'Theme: light',
	'theme.switch': 'Switch theme',
	'lang.label': 'Language',
	'app.tagline': 'Useful tools for web developers',
	'app.title': 'WebDev Useful Tools'
};

const ru: Dictionary = {
	'nav.xml': 'XML',
	'nav.json': 'JSON',
	'nav.url': 'URL',
	'nav.compare': 'Сравнить',
	'nav.share': 'Поделиться',
	'nav.rest': 'Rest client',
	'common.copy': 'Копировать в буфер обмена',
	'common.copied': 'Скопировано!',
	'common.download': 'Сохранить результат',
	'common.selectFile': 'Выбор файла',
	'common.noFile': 'Не выбран ни один файл',
	'common.format': 'Форматировать',
	'common.formatting': 'Форматирование…',
	'common.escape': 'Экранировать',
	'common.error': 'Что-то пошло не так',
	'formatter.source': 'Исходные данные',
	'formatter.output': 'Результат',
	'formatter.format': 'Форматировать',
	'formatter.unescape': 'Деэкранировать',
	'formatter.indent': '{n} отступа',
	'formatter.sourcePlaceholder': 'Вставьте XML сюда…',
	'formatter.sourcePlaceholderJson': 'Вставьте JSON сюда…',
	'formatter.outputPlaceholder': 'Здесь появится результат форматирования',
	'formatter.invalid': 'Не удалось обработать содержимое. Проверьте синтаксис.',
	'formatter.saved': 'Сохранено в {path}',
	'formatter.fileDone': 'Готово. Результат записан в {path}',
	'formatter.fileError': 'Ошибка обработки файла: {message}',
	'url.encode': 'Кодировать',
	'url.decode': 'Декодировать',
	'url.sourcePlaceholder': 'Вставьте URL или строку запроса…',
	'url.outputPlaceholder': 'Здесь появится результат',
	'compare.source': 'Исходные данные',
	'compare.result': 'Результат',
	'compare.compare': 'Сравнить',
	'compare.swap': 'Поменять стороны',
	'compare.identical': 'Стороны совпадают',
	'compare.summary': '{added} добавлено · {removed} удалено · {changed} изменено',
	'compare.placeholderLeft': 'Вставьте исходный текст…',
	'compare.placeholderRight': 'Вставьте текст для сравнения…',
	'share.title': 'Поделиться',
	'share.description':
		'Делитесь файлами и фрагментами в локальной сети. Загрузки хранятся в базе приложения, а ссылки открывают встроенный просмотрщик там, где установлено приложение.',
	'share.localOnly': 'Локальная сеть',
	'share.shareResult': 'Поделиться результатом',
	'share.server': 'Сервер обмена',
	'share.serverRunning': 'Сервер запущен',
	'share.serverError': 'Сервер недоступен',
	'share.serverUnavailable': 'Сервер обмена не запущен. Повторите запуск или перезапустите приложение.',
	'share.retry': 'Повторить запуск сервера',
	'share.retrying': 'Запуск сервера…',
	'share.draftHint': 'Содержимое из результата недоступно для правки. Измените его в инструменте XML или JSON.',
	'share.serverHint': 'Выберите адрес, доступный другим устройствам в этой сети.',
	'share.loopback': 'только это устройство',
	'share.loopbackHint': 'Сетевой интерфейс LAN не найден. Другие устройства не смогут подключиться.',
	'share.fileMode': 'Файл',
	'share.contentMode': 'Содержимое',
	'share.dropHint': 'Перетащите файл сюда или выберите его',
	'share.dropActive': 'Отпустите файл, чтобы поделиться',
	'share.limit': 'Максимум {size}',
	'share.uploadFile': 'Загрузить файл',
	'share.uploading': 'Загрузка…',
	'share.uploadContent': 'Загрузить содержимое',
	'share.filename': 'Имя файла',
	'share.language': 'Формат',
	'share.plainText': 'Текст',
	'share.contentPlaceholder': 'Вставьте содержимое для отправки…',
	'share.ready': 'Ссылка готова',
	'share.link': 'Ссылка для обмена',
	'share.copyLink': 'Копировать ссылку',
	'share.linkHint': 'Ссылка открывает просмотрщик в приложении. Размер файла: {size}.',
	'share.loading': 'Запуск сервера обмена…',
	'share.serverRequired': 'Сначала выберите сервер обмена.',
	'share.emptyContent': 'Добавьте содержимое перед загрузкой.',
	'share.tooLarge': 'Содержимое слишком большое. Максимум: {size}.',
	'share.desktopOnly': 'Обмен работает в настольном приложении, чтобы файлы оставались в вашей сети.',
	'viewer.back': 'Назад к обмену',
	'viewer.loading': 'Загрузка общего файла…',
	'viewer.retry': 'Повторить',
	'viewer.binary': 'Двоичный файл',
	'viewer.binaryHint': 'Этот файл нельзя показать. Скачайте его, чтобы открыть локально.',
	'viewer.filename': 'Имя файла',
	'viewer.type': 'Тип',
	'viewer.size': 'Размер',
	'viewer.created': 'Опубликован',
	'viewer.download': 'Скачать',
	'viewer.downloading': 'Скачивание…',
	'viewer.downloaded': 'Сохранено в {path}',
	'viewer.content': 'Общее содержимое',
	'viewer.desktopOnly': 'Просмотр общих файлов доступен только в настольном приложении.',
	'viewer.invalidLink': 'Ссылка недействительна или неполна.',
	'rest.title': 'Rest client',
	'rest.description':
		'Составляйте HTTP-запросы, изучайте ответы и храните историю вызовов API прямо в приложении. Раздел уже в разработке.',
	'rest.badge': 'Скоро',
	'theme.system': 'Тема: системная',
	'theme.dark': 'Тема: тёмная',
	'theme.light': 'Тема: светлая',
	'theme.switch': 'Переключить тему',
	'lang.label': 'Язык',
	'app.tagline': 'Полезные инструменты веб-разработчика',
	'app.title': 'Полезные инструменты веб-разработчика'
};

const dictionaries: Record<Locale, Dictionary> = { en, ru };

function detectLocale(): Locale {
	if (typeof window !== 'undefined') {
		const stored = window.localStorage.getItem('devtools.locale');
		if (stored === 'en' || stored === 'ru') return stored;
		const language = window.navigator.language || '';
		return language.toLowerCase().startsWith('ru') ? 'ru' : 'en';
	}
	return 'en';
}

export const locale = writable<Locale>(detectLocale());

if (typeof document !== 'undefined') {
	document.documentElement.lang = get(locale);
}

export function setLocale(next: Locale): void {
	locale.set(next);
	if (typeof window !== 'undefined') {
		window.localStorage.setItem('devtools.locale', next);
		document.documentElement.lang = next;
	}
}

export type Translate = (key: string, params?: Record<string, string | number>) => string;

export const t = derived(locale, ($locale) => {
	const dictionary = dictionaries[$locale];
	return (key: string, params?: Record<string, string | number>): string => {
		const template = dictionary[key] ?? en[key] ?? key;
		if (!params) return template;
		return template.replace(/\{(\w+)\}/g, (match, name: string) =>
			Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match
		);
	};
});
