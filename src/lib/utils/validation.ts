import type { Translate } from '$lib/stores/i18n';
import {
	validatePropertyField,
	validateRowName,
	type PropertyKind,
	type RestProperty,
	type ValidationIssue,
	type ValidationKind
} from './rest';

/**
 * The i18n key of every validation reason. Both request editors — Rest client and
 * the WSDL client — map a kind through this table, so one reason reads the same in
 * either tool. The keys interpolate `{name}` and `{char}`; `name-required` has no
 * placeholders and renders with the row name.
 */
export const VALIDATION_KEYS: Record<ValidationKind, string> = {
	'header-name': 'rest.validation.headerName',
	'header-value': 'rest.validation.headerValue',
	'cookie-name': 'rest.validation.cookieName',
	'cookie-value': 'rest.validation.cookieValue',
	'query-param': 'rest.validation.queryParam',
	'form-param': 'rest.validation.formParam',
	'multipart-field-name': 'rest.validation.multipartFieldName',
	'multipart-field-value': 'rest.validation.multipartFieldValue',
	'multipart-file-name': 'rest.validation.multipartFileFieldName',
	'file-missing': 'rest.validation.fileMissing',
	'name-required': 'rest.validation.nameRequired'
};

/** A localized reason for one issue. `t` is the caller's translate function. */
export function validationMessage(issue: ValidationIssue, t: Translate): string {
	return t(VALIDATION_KEYS[issue.kind], { name: issue.name || '—', char: issue.char });
}

/**
 * Cell-level reason for one property table: the mandatory-name rule first, then the
 * RFC rule of the cell. `index` is the row index, or `-1` for the add-row draft.
 * A caller skips this for a locked row — its value is written by the tool.
 */
export function propertyFieldHint(
	kind: PropertyKind,
	field: 'name' | 'value',
	row: RestProperty,
	index: number,
	t: Translate
): string | null {
	if (field === 'name') {
		const missing = validateRowName(row);
		if (missing) return validationMessage({ ...missing, row: index }, t);
	}
	const issue = validatePropertyField(kind, field, row[field]);
	if (!issue) return null;
	return validationMessage({ ...issue, name: row.name.trim() || row[field], row: index }, t);
}

/** Why a request cannot be sent: the first issue wrapped into the blocked message. */
export function blockedMessage(issues: ValidationIssue[], t: Translate): string | null {
	if (!issues.length) return null;
	return t('rest.validation.blocked', { detail: validationMessage(issues[0], t) });
}
