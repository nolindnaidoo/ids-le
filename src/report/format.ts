import * as vscode from 'vscode';
import { counts, type Found, KIND_NAMES } from '../extract';

export interface ReportInput {
	readonly file: string;
	readonly format: string;
	readonly rows: readonly Found[];
}

/**
 * The report a person reads, as Markdown: the named identifiers grouped by
 * kind, then every run that could not be named with the reason — never
 * dropped, because a reader counting only the named ones would understate the
 * document.
 */
export function formatReport({ file, format, rows }: ReportInput): string {
	const [named, refused] = counts(rows);
	const lines: string[] = [`# ${vscode.l10n.t('IDs-LE report')}`, ''];
	lines.push(
		`${code(file)} · ${format} · ${vscode.l10n.t('{0} named, {1} could not be named', named, refused)}`,
		'',
	);
	if (rows.length === 0) {
		lines.push(vscode.l10n.t('No identifiers found.'), '');
		return lines.join('\n');
	}

	for (const kind of KIND_NAMES) {
		const ofKind = rows.filter((row) => row.valid && row.kind === kind);
		if (ofKind.length === 0) continue;
		lines.push(`## ${kind} (${ofKind.length})`, '');
		for (const row of ofKind) lines.push(item(row));
		lines.push('');
	}

	const refusals = rows.filter((row) => !row.valid);
	if (refusals.length > 0) {
		lines.push(
			`## ${vscode.l10n.t('Could not be named ({0})', refusals.length)}`,
			'',
		);
		for (const row of refusals) {
			lines.push(
				item(row),
				'',
				`  ${row.refused ?? ''}: ${row.detail ?? ''}`,
				'',
			);
		}
	}
	return lines.join('\n');
}

/** One row: where, what, under which key, and what was decoded. */
function item(row: Found): string {
	const parts = [`**${row.line}:${row.column}**`, code(row.value)];
	if (!row.valid && row.kind) parts.push(row.kind);
	if (row.key !== undefined)
		parts.push(`${vscode.l10n.t('key')} ${code(row.key)}`);
	if (row.version !== undefined) parts.push(`v${row.version}`);
	if (row.variant !== undefined && row.variant !== 'rfc4122')
		parts.push(row.variant);
	if (row.timestamp !== undefined) parts.push(row.timestamp);
	return `- ${parts.join(' · ')}`;
}

/** Text as a code span. A code span cannot escape a backtick, so one becomes a quote. */
function code(text: string): string {
	return `\`${text.replace(/`/g, "'").replace(/\r?\n/g, ' ')}\``;
}
