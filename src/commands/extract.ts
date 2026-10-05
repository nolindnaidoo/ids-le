import * as vscode from 'vscode';
import { readConfig } from '../config/config';
import { counts, extract, resolveFormat } from '../extract';
import { formatReport } from '../report/format';
import type { CommandDependencies } from './index';
import { showReport } from './output';

/**
 * Extract the identifiers in the active document, as the editor holds it.
 *
 * The format comes from the language mode first and the file name second; it
 * decides only the key paths, never which runs are found. Now is read here,
 * once, and handed to the engine.
 */
export async function extractFromActiveDocument(
	deps: CommandDependencies,
): Promise<void> {
	deps.telemetry.event('command', { name: 'extract' });
	const editor = vscode.window.activeTextEditor;
	if (!editor) {
		deps.notifier.error(vscode.l10n.t('No active editor'));
		return;
	}
	const document = editor.document;
	const text = document.getText();
	if (text.length === 0) {
		deps.notifier.info(vscode.l10n.t('File is empty'));
		return;
	}
	const config = readConfig();
	if (config.safetyEnabled && !document.isUntitled) {
		try {
			const { size } = await vscode.workspace.fs.stat(document.uri);
			if (size > config.safetyFileSizeWarnBytes) {
				deps.notifier.warn(
					vscode.l10n.t(
						'Large file detected ({0} bytes). Extraction may take longer.',
						size,
					),
				);
			}
		} catch {
			// A document with no file behind it has no size to warn about.
		}
	}

	// The base name, not the path: `.env.local` is dotenv by its own name, and
	// the rule that says so never sees a directory in front of it.
	const base = document.fileName.slice(
		Math.max(
			document.fileName.lastIndexOf('/'),
			document.fileName.lastIndexOf('\\'),
		) + 1,
	);
	const format = resolveFormat(document.languageId, base);
	const rows = extract(text, format, {
		clock: { nowMs: Date.now() },
		kind: config.kind === 'all' ? undefined : config.kind,
	});
	const [named, refused] = counts(rows);
	const file = document.isUntitled
		? document.fileName
		: vscode.workspace.asRelativePath(document.uri, false);
	await showReport(
		formatReport({ file, format, rows, positions: config.showPositions }),
		config,
		deps,
		formatReport({
			file,
			format,
			rows,
			positions: config.clipboardIncludesPositions,
		}),
	);

	deps.telemetry.event('extracted', {
		named: String(named),
		refused: String(refused),
	});
	deps.statusBar.flash(vscode.l10n.t('{0} identifier(s)', named));
	if (refused > 0) {
		deps.notifier.warn(
			vscode.l10n.t(
				'{0} run(s) could not be named; the report gives each reason',
				refused,
			),
		);
	}
}
