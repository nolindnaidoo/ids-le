import { beforeEach, describe, expect, it } from 'vitest';
import {
	_clipboardText,
	_createDocument,
	_createExtensionContext,
	_diagnostics,
	_openedDocuments,
	_registeredCommands,
	_resetMockState,
	_respondToOpenDialog,
	_setActiveEditor,
	_setConfig,
	_setWorkspaceFiles,
	_shownMessages,
	executedBuiltins,
	Uri,
	workspace,
} from '../__mocks__/vscode';
import { registerOpenSettingsCommand } from '../config/settings';
import type { Telemetry } from '../telemetry/telemetry';
import { createNotifier } from '../ui/notifier';
import type { StatusBar } from '../ui/statusBar';
import { generateHelpContent, registerHelpCommand } from './help';
import { registerCommands } from './index';

function makeDeps() {
	const flashes: string[] = [];
	const telemetry: Telemetry = { event: () => {}, dispose: () => {} };
	const statusBar: StatusBar = { flash: (text) => flashes.push(text) };
	return {
		deps: {
			notifier: createNotifier(),
			statusBar,
			telemetry,
			ratingPrompt: { recordSuccess: async () => {} },
		},
		flashes,
	};
}

async function runCommand(id: string, ...args: unknown[]): Promise<void> {
	const handler = _registeredCommands().get(id);
	if (!handler) throw new Error(`command not registered: ${id}`);
	await handler(...args);
}

function report(): string {
	const last = _openedDocuments().at(-1);
	if (!last) throw new Error('no report was opened');
	return last.getText();
}

const DOCUMENT = JSON.stringify(
	{
		service: {
			requestId: '019ff344-cc00-7abc-8def-0123456789ab',
			id: 'f47ac10b-58cc-4372-a567-0e02b2c3d479',
		},
		digest: '5d41402abc4b2a76b9719d911017c592',
	},
	null,
	2,
);

let flashes: string[] = [];
beforeEach(() => {
	_resetMockState();
	const made = makeDeps();
	flashes = made.flashes;
	registerCommands(_createExtensionContext() as never, made.deps);
});

describe('ids-le.extract', () => {
	it('errors when no editor is active', async () => {
		await runCommand('ids-le.extract');
		expect(_shownMessages()[0]).toMatchObject({
			kind: 'error',
			message: 'No active editor',
		});
	});

	it('says an empty file is empty', async () => {
		_setConfig('ids-le.notificationsLevel', 'all');
		_setActiveEditor(_createDocument({ content: '' }));
		await runCommand('ids-le.extract');
		expect(_shownMessages()[0]).toMatchObject({
			kind: 'info',
			message: 'File is empty',
		});
	});

	it('names each identifier with its position, key path and decoded time, and refuses the digest by name', async () => {
		_setActiveEditor(
			_createDocument({
				content: DOCUMENT,
				languageId: 'json',
				fileName: '/w/config.json',
			}),
		);
		await runCommand('ids-le.extract');
		const text = report();
		expect(text).toContain(
			'`/w/config.json` · json · 2 named, 1 could not be named',
		);
		expect(text).toContain('## uuid (2)');
		expect(text).toContain(
			'- **3:19** · `019ff344-cc00-7abc-8def-0123456789ab` · key `service.requestId` · v7 · 2026-08-12T00:00:00.000Z',
		);
		expect(text).toContain('## Could not be named (1)');
		expect(text).toContain(
			'ambiguous_kind: 32 hex digits are an unhyphenated UUID and an MD5 digest',
		);
		expect(flashes).toContain('2 identifier(s)');
	});

	it('reads the format from the file name when the language mode does not say', async () => {
		_setActiveEditor(
			_createDocument({
				content: 'USER_ID=6a7bb780a1b2c3d4e5f60718\n',
				fileName: '/w/.env.local',
			}),
		);
		await runCommand('ids-le.extract');
		expect(report()).toContain('## objectid (1)');
	});

	it('narrows to the kind the setting names, and keeps every refusal', async () => {
		_setConfig('ids-le.kind', 'ulid');
		_setActiveEditor(
			_createDocument({ content: DOCUMENT, languageId: 'json' }),
		);
		await runCommand('ids-le.extract');
		expect(report()).not.toContain('## uuid');
		expect(report()).toContain('## Could not be named (1)');
	});

	it('warns about a large file before extracting', async () => {
		_setConfig('ids-le.notificationsLevel', 'all');
		_setConfig('ids-le.safety.fileSizeWarnBytes', 1000);
		_setWorkspaceFiles({ '/w/big.txt': 'x'.repeat(2000) });
		_setActiveEditor(
			_createDocument({ content: 'x'.repeat(2000), fileName: '/w/big.txt' }),
		);
		await runCommand('ids-le.extract');
		expect(
			_shownMessages().some((m) =>
				m.message.startsWith('Large file detected (2000 bytes)'),
			),
		).toBe(true);
	});

	it('copies the report when asked to', async () => {
		_setConfig('ids-le.copyToClipboardEnabled', true);
		_setActiveEditor(
			_createDocument({ content: DOCUMENT, languageId: 'json' }),
		);
		await runCommand('ids-le.extract');
		expect(_clipboardText()).toBe(report());
	});

	it('shows no positions when the setting is off', async () => {
		_setConfig('ids-le.showPositions', false);
		_setActiveEditor(
			_createDocument({ content: DOCUMENT, languageId: 'json' }),
		);
		await runCommand('ids-le.extract');
		expect(report()).not.toMatch(/\*\*\d+:\d+\*\*/);
		expect(report()).toContain('`f47ac10b-58cc-4372-a567-0e02b2c3d479`');
	});

	it('decides positions for the clipboard separately from the report', async () => {
		_setConfig('ids-le.copyToClipboardEnabled', true);
		_setConfig('ids-le.clipboardIncludesPositions', false);
		_setActiveEditor(
			_createDocument({ content: DOCUMENT, languageId: 'json' }),
		);
		await runCommand('ids-le.extract');
		expect(report()).toMatch(/\*\*\d+:\d+\*\*/);
		expect(_clipboardText()).not.toMatch(/\*\*\d+:\d+\*\*/);
		expect(_clipboardText()).toContain(
			'`f47ac10b-58cc-4372-a567-0e02b2c3d479`',
		);
	});
});

describe('settings and help', () => {
	it('opens the settings filtered to this extension', async () => {
		const { deps } = makeDeps();
		registerOpenSettingsCommand(
			_createExtensionContext() as never,
			deps.telemetry,
		);
		await runCommand('ids-le.openSettings');
		expect(executedBuiltins.at(-1)).toMatchObject({ args: ['ids-le.'] });
	});

	it('opens the help, which names every kind and every refusal', async () => {
		const { deps } = makeDeps();
		registerHelpCommand(_createExtensionContext() as never, deps.telemetry);
		await runCommand('ids-le.help');
		for (const word of [
			'uuid',
			'ulid',
			'nanoid',
			'objectid',
			'snowflake',
			'ambiguous_kind',
			'malformed',
			'nil_or_max',
			'version_claim_mismatch',
			'timestamp_implausible',
		]) {
			expect(generateHelpContent()).toContain(word);
		}
	});
});

describe('ids-le.scanWorkspace and ids-le.scanFolder', () => {
	const UUID = 'f47ac10b-58cc-4372-a567-0e02b2c3d479';
	const BAD = 'f47ac10b-58cc-4372-1567-0e02b2c3d479';
	const TREE = {
		'/w/api/a.json': JSON.stringify({ id: UUID }),
		'/w/api/b.txt': `first ${UUID}\nthen ${BAD}`,
		'/w/empty.md': 'nothing here',
		'/w/node_modules/dep.json': JSON.stringify({ id: UUID }),
		// No extension to go by, so it is read and found not to be text.
		'/w/blob': new Uint8Array([0x89, 0x50, 0x00, 0x47]),
	};

	function open(): void {
		_setWorkspaceFiles(TREE);
		workspace.workspaceFolders = [{ uri: Uri.file('/w'), name: 'w', index: 0 }];
	}

	it('warns when no workspace is open', async () => {
		_setConfig('ids-le.notificationsLevel', 'all');
		await runCommand('ids-le.scanWorkspace');
		expect(_shownMessages()[0]).toMatchObject({ kind: 'warning' });
		expect(_openedDocuments()).toHaveLength(0);
	});

	it('reports every file that holds an identifier, one section each, in path order', async () => {
		open();
		await runCommand('ids-le.scanWorkspace');

		const text = report();
		expect(text).toContain('# IDs-LE workspace report');
		expect(text).toContain('3 file(s) read · 2 named, 1 could not be named');
		// The table names every file that holds something, with both counts.
		expect(text).toContain('| File | Named | Could not be named |');
		expect(text).toContain('| `/w/api/a.json` | 1 | 0 |');
		expect(text).toContain('| `/w/api/b.txt` | 1 | 1 |');
		// What could not be named is counted there and not listed below.
		expect(text.match(/^## .*$/gm)).toEqual([
			'## `/w/api/a.json` · json (1)',
			'## `/w/api/b.txt` · text (1)',
		]);
		expect(text).not.toContain(BAD);
		expect(text).toContain('`ids-le.workspace.scanIncludeRefusals`');
		// A named identifier says its kind here, since nothing groups by it.
		expect(text).toContain(`- **1:8** · \`${UUID}\` · uuid`);
		// Left out by the built-in excludes, and the report says they were on.
		expect(text).not.toContain('node_modules');
		expect(text).toContain(
			'> Not read: dependency folders, build output, caches and lockfiles; images, fonts, archives and other binary files; 0 file(s) ignored by .gitignore. The `ids-le.workspace.*` settings change this.',
		);
		expect(text).toContain(
			'> 1 file(s) that are not UTF-8 text were not read.',
		);
		expect(flashes).toEqual(['2 identifier(s) in 2 file(s)']);
	});

	it('lists each run that could not be named when asked to', async () => {
		open();
		_setConfig('ids-le.workspace.scanIncludeRefusals', true);
		await runCommand('ids-le.scanWorkspace');

		expect(report()).toContain('## `/w/api/b.txt` · text (2)');
		expect(report()).toContain(BAD);
		expect(report()).not.toContain('scanIncludeRefusals');
	});

	it('leaves the Problems panel alone unless asked', async () => {
		open();
		await runCommand('ids-le.scanWorkspace');
		expect(_diagnostics().size).toBe(0);
	});

	it('puts the runs that could not be named in the Problems panel when asked, and only those', async () => {
		open();
		_setConfig('ids-le.workspace.scanProblemsEnabled', true);
		await runCommand('ids-le.scanWorkspace');

		const problems = _diagnostics();
		expect([...problems.keys()]).toEqual(['/w/api/b.txt']);
		const [problem] = problems.get('/w/api/b.txt') ?? [];
		expect(problem?.severity).toBe(1);
		expect(problem?.source).toBe('ids-le');
		expect(problem?.range.start).toMatchObject({ line: 1, character: 5 });
		expect(problem?.range.end.character).toBe(5 + BAD.length);
	});

	it('scans only the folder it is handed', async () => {
		open();
		_setWorkspaceFiles({ ...TREE, '/w/web/c.txt': UUID });
		await runCommand('ids-le.scanFolder', Uri.file('/w/web'));

		expect(report()).toContain('`/w/web` · 1 file(s) read · 1 named');
		// Paths are relative to the folder that was picked.
		expect(report().match(/^## .*$/gm)).toEqual(['## `c.txt` · text (1)']);
	});

	it('asks for a folder from the palette, and does nothing when none is picked', async () => {
		open();
		_respondToOpenDialog(() => undefined);
		await runCommand('ids-le.scanFolder');
		expect(_openedDocuments()).toHaveLength(0);

		_respondToOpenDialog(() => [Uri.file('/w/api')]);
		await runCommand('ids-le.scanFolder');
		expect(report()).toContain('`/w/api` · 2 file(s) read');
	});

	it('stops at the results limit and says the rest was not read', async () => {
		open();
		_setConfig('ids-le.workspace.scanMaxResults', 1);
		await runCommand('ids-le.scanWorkspace');

		const text = report();
		expect(text.match(/^## .*$/gm)).toEqual(['## `/w/api/a.json` · json (1)']);
		expect(text).toContain(
			'> The results limit was reached. The rest of the files were not read.',
		);
	});

	it('says when more files matched than the file limit', async () => {
		open();
		_setConfig('ids-le.workspace.scanMaxFiles', 1);
		await runCommand('ids-le.scanWorkspace');
		expect(report()).toContain(
			'> More files matched than the limit of 1. The rest were not read.',
		);
	});

	it('honours the positions settings as Extract does', async () => {
		open();
		_setConfig('ids-le.showPositions', false);
		await runCommand('ids-le.scanWorkspace');
		expect(report()).not.toMatch(/\*\*\d+:\d+\*\*/);
		expect(report()).toContain(`- \`${UUID}\` · uuid`);
	});
});
