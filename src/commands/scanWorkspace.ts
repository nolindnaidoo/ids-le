import * as vscode from 'vscode';
import { readConfig } from '../config/config';
import { extract, type Found, resolveFormat } from '../extract';
import { type FileRows, formatWorkspaceReport } from '../report/format';
import {
	listFiles,
	type ScanLimits,
	type ScanSummary,
	scanFiles,
} from '../workspace/scan';
import type { CommandDependencies } from './index';
import { showReport } from './output';

/**
 * Extract the identifiers in every file under a folder, or in the whole
 * workspace when no folder is given.
 *
 * Files are read from disk, so an unsaved edit is not seen: this reports what
 * the project holds, where Extract reports what the editor holds.
 */
export async function scanWorkspace(
	deps: CommandDependencies,
	diagnostics: vscode.DiagnosticCollection,
	root?: vscode.Uri,
): Promise<void> {
	deps.telemetry.event('command', {
		name: root === undefined ? 'scanWorkspace' : 'scanFolder',
	});
	if (
		root === undefined &&
		(vscode.workspace.workspaceFolders ?? []).length === 0
	) {
		deps.notifier.warn(
			vscode.l10n.t('No workspace open. Please open a workspace folder first.'),
		);
		return;
	}
	const config = readConfig();
	const limits: ScanLimits = {
		patterns: config.workspaceScanPatterns,
		excludes: config.workspaceScanExcludes,
		maxFiles: config.workspaceScanMaxFiles,
		maxFileBytes: config.safetyEnabled
			? config.safetyFileSizeWarnBytes
			: undefined,
		respectGitignore: config.workspaceScanRespectGitignore,
	};

	await vscode.window.withProgress(
		{
			location: vscode.ProgressLocation.Notification,
			title: vscode.l10n.t('Scanning files...'),
			cancellable: true,
		},
		async (progress, token) => {
			const { files, fileLimitReached } = await listFiles(root, limits);
			const found: Scanned[] = [];
			const nowMs = Date.now();
			let total = 0;
			const scanned = await scanFiles(
				root,
				files,
				limits,
				token,
				(done, all) =>
					progress.report({
						message: vscode.l10n.t('{0} of {1} files', done, all),
					}),
				({ uri, file, text }) => {
					const format = resolveFormat(undefined, baseName(file));
					const all = extract(text, format, {
						clock: { nowMs },
						kind: config.kind === 'all' ? undefined : config.kind,
					});
					if (all.length === 0) return true;
					const named = all.filter((row) => row.valid);
					// The limit is on what the report lists. A lockfile's
					// thousand digests are counted either way.
					const rows = (
						config.workspaceScanIncludeRefusals ? all : named
					).slice(0, config.workspaceScanMaxResults - total);
					found.push({
						uri,
						file,
						format,
						rows,
						named: named.length,
						refused: all.length - named.length,
						refusals: all.filter((row) => !row.valid),
					});
					total += rows.length;
					return total < config.workspaceScanMaxResults;
				},
			);
			// A cancelled scan read part of the tree. Reporting that as the
			// project's identifiers would understate it without saying so.
			if (scanned.cancelled) return;
			const summary: ScanSummary = { ...scanned, fileLimitReached };

			// Each scan replaces the last one's problems, and a scan that
			// publishes none still clears them.
			publish(diagnostics, config.workspaceScanProblemsEnabled ? found : []);
			const where =
				root === undefined
					? undefined
					: vscode.workspace.asRelativePath(root, false);
			const report = (positions: boolean): string =>
				formatWorkspaceReport({
					where,
					files: found,
					summary,
					limits,
					refusalsListed: config.workspaceScanIncludeRefusals,
					positions,
				});
			await showReport(
				report(config.showPositions),
				config,
				deps,
				report(config.clipboardIncludesPositions),
			);

			const named = found.reduce((sum, entry) => sum + entry.named, 0);
			const refused = found.reduce((sum, entry) => sum + entry.refused, 0);
			deps.telemetry.event('workspace-scanned', {
				files: String(summary.read),
				named: String(named),
				refused: String(refused),
			});
			deps.statusBar.flash(
				vscode.l10n.t('{0} identifier(s) in {1} file(s)', named, found.length),
			);
			if (refused > 0) {
				deps.notifier.warn(
					vscode.l10n.t(
						'{0} run(s) could not be named; the report gives each reason',
						refused,
					),
				);
			}
		},
	);
}

/** Scan the folder picked in the Explorer, or ask for one. */
export async function scanFolder(
	deps: CommandDependencies,
	diagnostics: vscode.DiagnosticCollection,
	picked?: vscode.Uri,
): Promise<void> {
	const folder = picked ?? (await askForFolder());
	if (folder === undefined) return;
	await scanWorkspace(deps, diagnostics, folder);
}

async function askForFolder(): Promise<vscode.Uri | undefined> {
	const start = vscode.workspace.workspaceFolders?.[0]?.uri;
	const chosen = await vscode.window.showOpenDialog({
		canSelectFiles: false,
		canSelectFolders: true,
		canSelectMany: false,
		...(start === undefined ? {} : { defaultUri: start }),
		openLabel: vscode.l10n.t('Scan Folder'),
	});
	return chosen?.[0];
}

type Scanned = FileRows & {
	readonly uri: vscode.Uri;
	readonly refusals: readonly Found[];
};

/**
 * The runs that could not be named, in the Problems panel.
 *
 * Only those: a named identifier is not a problem. And only when asked for:
 * across a project they run to thousands, mostly digests in generated files,
 * and a panel full of them hides whatever else is there.
 */
function publish(
	diagnostics: vscode.DiagnosticCollection,
	found: readonly Scanned[],
): void {
	diagnostics.clear();
	for (const { uri, refusals } of found) {
		if (refusals.length === 0) continue;
		diagnostics.set(uri, refusals.map(problem));
	}
}

function problem(row: Found): vscode.Diagnostic {
	const start = new vscode.Position(row.line - 1, row.column - 1);
	const diagnostic = new vscode.Diagnostic(
		new vscode.Range(start, start.translate(0, row.value.length)),
		`${row.refused ?? ''}: ${row.detail ?? ''}`,
		vscode.DiagnosticSeverity.Warning,
	);
	diagnostic.source = 'ids-le';
	return diagnostic;
}

/** The base name, not the path: `.env.local` is dotenv by its own name. */
function baseName(file: string): string {
	return file.slice(
		Math.max(file.lastIndexOf('/'), file.lastIndexOf('\\')) + 1,
	);
}
