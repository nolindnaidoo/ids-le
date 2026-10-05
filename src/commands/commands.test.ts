import { beforeEach, describe, expect, it } from 'vitest';
import {
	_clipboardText,
	_createDocument,
	_createExtensionContext,
	_openedDocuments,
	_registeredCommands,
	_resetMockState,
	_setActiveEditor,
	_setConfig,
	_setWorkspaceFiles,
	_shownMessages,
	executedBuiltins,
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
		deps: { notifier: createNotifier(), statusBar, telemetry },
		flashes,
	};
}

async function runCommand(id: string): Promise<void> {
	const handler = _registeredCommands().get(id);
	if (!handler) throw new Error(`command not registered: ${id}`);
	await handler();
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
