import * as vscode from 'vscode';
import type { Telemetry } from '../telemetry/telemetry';

/**
 * Register the help command
 */
export function registerHelpCommand(
	context: vscode.ExtensionContext,
	telemetry: Telemetry,
): void {
	const disposable = vscode.commands.registerCommand(
		'ids-le.help',
		async () => {
			telemetry.event('command', { name: 'help' });
			await showHelp();
		},
	);
	context.subscriptions.push(disposable);
}

async function showHelp(): Promise<void> {
	const doc = await vscode.workspace.openTextDocument({
		content: generateHelpContent(),
		language: 'markdown',
	});
	await vscode.window.showTextDocument(doc, {
		preview: false,
		viewColumn: vscode.ViewColumn.Beside,
	});
}

/** Every claim here is the crate's behaviour, and the corpus pins it. */
export function generateHelpContent(): string {
	return [
		'# IDs-LE Help',
		'',
		'Finds every identifier in a document — UUID, ULID, NanoID, MongoDB ObjectId and Snowflake — with its position, its key path, whether it is valid and, where it embeds one, the time it was minted. A run that cannot be named honestly is reported with the reason, never dropped.',
		'',
		'## Commands',
		'',
		'- **Extract IDs** (`Ctrl+Alt+I`, Mac `Cmd+Alt+I`): the active document, as the editor holds it.',
		'',
		'## Kinds',
		'',
		'| Kind | Shape | Decoded time |',
		'|---|---|---|',
		'| `uuid` | `8-4-4-4-12` hex, any version | v1, v6, v7 |',
		'| `ulid` | 26 Crockford base32 | always |',
		'| `nanoid` | 21 of `A-Za-z0-9_-` | never |',
		'| `objectid` | 24 hex, under a key naming an id | always |',
		'| `snowflake` | 17–19 digits, under a key naming an id | always (Twitter or Discord epoch) |',
		'',
		'## Refusals',
		'',
		'| Reason | When |',
		'|---|---|',
		'| `ambiguous_kind` | Two schemes fit and nothing in the document chooses — 32 hex digits are a UUID and an MD5 digest alike. |',
		'| `malformed` | The right shape, failing validation. |',
		'| `nil_or_max` | The nil or max UUID, which name nothing. |',
		'| `version_claim_mismatch` | A v4 UUID with twelve zero nibbles in a row. |',
		'| `timestamp_implausible` | A decoded time before 1990 or more than a year from now. |',
		'',
		"An ObjectId and a Snowflake have no structure to check, so they are named only when the field's own key ends in `id` — `_id`, `userId`, `USER-ID`. In a document with no keys, such as plain text, they are refused for want of that evidence.",
		'',
		'## Key paths',
		'',
		'In JSON, YAML, TOML, INI, `.env`, CSV and TSV each identifier also carries the key path it sits under. The format decides only how an identifier is addressed, never whether it is found.',
		'',
		'## Agents',
		'',
		"The bundled MCP server offers `extract_ids` to agent mode. It answers exactly as the `ids-le` command-line tool's server does.",
		'',
		'## Troubleshooting',
		'',
		'- **An ObjectId came back refused**: rename the field so its key ends in `id`, or read the reason beside it.',
		'- **Only some kinds appear**: `ids-le.kind` narrows the named identifiers; refusals are always shown.',
		'',
	].join('\n');
}
