import * as vscode from 'vscode';

/**
 * Reading many files from disk, for a command that runs over a folder or the
 * whole workspace.
 *
 * This file is the same in every extension of the family and names none of
 * them: what to do with a file's text is the caller's business. What is here
 * is what must not differ between them — which files are read, in what order,
 * what is skipped and how that is counted.
 */

export interface ScanLimits {
	/** Globs of the files to read, relative to the root. */
	readonly patterns: readonly string[];
	/** Globs of the files to leave out. */
	readonly excludes: readonly string[];
	/** The most files one scan reads. */
	readonly maxFiles: number;
	/** A file larger than this is not read. Undefined reads any size. */
	readonly maxFileBytes: number | undefined;
}

export interface ScannedFile {
	readonly uri: vscode.Uri;
	/** The path as a report shows it: relative to the workspace. */
	readonly file: string;
	readonly text: string;
}

/** What a scan did, so a report can say what it did not look at. */
export interface ScanSummary {
	/** Files whose text was handed to the caller. */
	readonly read: number;
	/** Files left unread for being over the size limit. */
	readonly tooLarge: number;
	/** Files left unread for not being UTF-8 text, or not being readable. */
	readonly notText: number;
	/** More files matched than `maxFiles` allows. */
	readonly fileLimitReached: boolean;
	/** The caller asked to stop before the last file. */
	readonly stoppedEarly: boolean;
	readonly cancelled: boolean;
}

/**
 * The files a scan would read, in a stable order.
 *
 * `findFiles` promises no order, so two scans of one tree would list files
 * differently. The comparison is plain rather than `localeCompare`: the order
 * must not change with the editor's display language.
 */
export async function listFiles(
	root: vscode.Uri | undefined,
	limits: ScanLimits,
): Promise<{ files: vscode.Uri[]; fileLimitReached: boolean }> {
	const exclude =
		limits.excludes.length === 0 ? undefined : `{${limits.excludes.join(',')}}`;
	const seen = new Set<string>();
	const out: vscode.Uri[] = [];
	for (const pattern of limits.patterns) {
		const include =
			root === undefined ? pattern : new vscode.RelativePattern(root, pattern);
		// One more than the limit, which is how a scan knows it was cut short.
		for (const uri of await vscode.workspace.findFiles(
			include,
			exclude,
			limits.maxFiles + 1,
		)) {
			const key = uri.toString();
			if (seen.has(key)) continue;
			seen.add(key);
			out.push(uri);
		}
	}
	out.sort((a, b) => (a.path < b.path ? -1 : Number(a.path > b.path)));
	return {
		files: out.slice(0, limits.maxFiles),
		fileLimitReached: out.length > limits.maxFiles,
	};
}

/**
 * A file's text, or undefined when it is not UTF-8 text.
 *
 * Read as bytes and decoded here rather than opened as a document: an editor
 * will open a binary or a UTF-16 file as something, and results read out of a
 * mis-decode are invented.
 */
export function decodeText(bytes: Uint8Array): string | undefined {
	if (bytes.includes(0)) return undefined;
	try {
		return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
	} catch {
		return undefined;
	}
}

/**
 * Hand each readable file's text to `each`, in order.
 *
 * `each` returns false to stop the scan, which is how a caller enforces a
 * limit on its own results.
 */
export async function scanFiles(
	files: readonly vscode.Uri[],
	limits: ScanLimits,
	token: vscode.CancellationToken,
	onProgress: (done: number, total: number) => void,
	each: (file: ScannedFile) => boolean | undefined,
): Promise<Omit<ScanSummary, 'fileLimitReached'>> {
	let read = 0;
	let tooLarge = 0;
	let notText = 0;
	for (const [index, uri] of files.entries()) {
		if (token.isCancellationRequested) {
			return { read, tooLarge, notText, stoppedEarly: false, cancelled: true };
		}
		if (index % 50 === 0) onProgress(index, files.length);

		let text: string | undefined;
		try {
			if (
				limits.maxFileBytes !== undefined &&
				(await vscode.workspace.fs.stat(uri)).size > limits.maxFileBytes
			) {
				tooLarge++;
				continue;
			}
			text = decodeText(await vscode.workspace.fs.readFile(uri));
		} catch {
			// A file that could not be read was not examined, and is counted
			// with the others nothing could be read from.
			text = undefined;
		}
		if (text === undefined) {
			notText++;
			continue;
		}

		read++;
		const file = vscode.workspace.asRelativePath(uri, false);
		if (each({ uri, file, text }) === false) {
			return {
				read,
				tooLarge,
				notText,
				stoppedEarly: index < files.length - 1,
				cancelled: false,
			};
		}
	}
	return { read, tooLarge, notText, stoppedEarly: false, cancelled: false };
}

/** The lines a report adds for whatever a scan left unread. Empty when it read everything. */
export function unreadNotes(
	summary: ScanSummary,
	limits: ScanLimits,
): string[] {
	const notes: string[] = [];
	if (summary.fileLimitReached) {
		notes.push(
			vscode.l10n.t(
				'More files matched than the limit of {0}. The rest were not read.',
				limits.maxFiles,
			),
		);
	}
	if (summary.stoppedEarly) {
		notes.push(
			vscode.l10n.t(
				'The results limit was reached. The rest of the files were not read.',
			),
		);
	}
	if (summary.tooLarge > 0) {
		notes.push(
			vscode.l10n.t(
				'{0} file(s) larger than the safety limit were not read.',
				summary.tooLarge,
			),
		);
	}
	if (summary.notText > 0) {
		notes.push(
			vscode.l10n.t(
				'{0} file(s) that are not UTF-8 text were not read.',
				summary.notText,
			),
		);
	}
	return notes;
}
