import * as vscode from 'vscode';
import { counts, type Found, KIND_NAMES } from '../extract';
import {
	type ScanLimits,
	type ScanSummary,
	unreadNotes,
} from '../workspace/scan';

export interface ReportInput {
	readonly file: string;
	readonly format: string;
	readonly rows: readonly Found[];
	/** Whether each row leads with its line and column. On unless said otherwise. */
	readonly positions?: boolean;
}

/**
 * The report a person reads, as Markdown: the named identifiers grouped by
 * kind, then every run that could not be named with the reason — never
 * dropped, because a reader counting only the named ones would understate the
 * document.
 */
export function formatReport({
	file,
	format,
	rows,
	positions = true,
}: ReportInput): string {
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
		for (const row of ofKind) lines.push(item(row, positions));
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
				item(row, positions),
				'',
				`  ${row.refused ?? ''}: ${row.detail ?? ''}`,
				'',
			);
		}
	}
	return lines.join('\n');
}

export interface FileRows {
	readonly file: string;
	readonly format: string;
	readonly rows: readonly Found[];
}

export interface WorkspaceReportInput {
	/** The folder that was scanned, or undefined for the whole workspace. */
	readonly where: string | undefined;
	readonly files: readonly FileRows[];
	readonly summary: ScanSummary;
	readonly limits: ScanLimits;
	readonly positions?: boolean;
}

/**
 * The report for a folder or a workspace: one section per file that holds an
 * identifier, in path order, then whatever the scan left unread. A file with
 * nothing in it is counted and not listed.
 */
export function formatWorkspaceReport({
	where,
	files,
	summary,
	limits,
	positions = true,
}: WorkspaceReportInput): string {
	const [named, refused] = counts(files.flatMap((entry) => entry.rows));
	const lines: string[] = [
		`# ${vscode.l10n.t('{0} workspace report', 'IDs-LE')}`,
		'',
	];
	const scope = where === undefined ? '' : `${code(where)} · `;
	lines.push(
		`${scope}${vscode.l10n.t('{0} file(s) read', summary.read)} · ${vscode.l10n.t('{0} named, {1} could not be named', named, refused)}`,
		'',
	);
	if (files.length === 0)
		lines.push(vscode.l10n.t('No identifiers found.'), '');

	for (const entry of files) {
		lines.push(
			`## ${code(entry.file)} · ${entry.format} (${entry.rows.length})`,
			'',
		);
		for (const row of entry.rows) {
			lines.push(item(row, positions, true));
			if (!row.valid)
				lines.push('', `  ${row.refused ?? ''}: ${row.detail ?? ''}`, '');
		}
		lines.push('');
	}

	const notes = unreadNotes(summary, limits);
	if (notes.length > 0) lines.push(...notes.map((note) => `> ${note}`), '');
	return lines.join('\n');
}

/**
 * One row: where, if asked for, then what, under which key, and what was
 * decoded. `withKind` names the kind of a named identifier too, for a report
 * that does not already group by it.
 */
function item(row: Found, positions: boolean, withKind = false): string {
	const parts = positions
		? [`**${row.line}:${row.column}**`, code(row.value)]
		: [code(row.value)];
	if ((withKind || !row.valid) && row.kind) parts.push(row.kind);
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
