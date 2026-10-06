import * as assert from 'node:assert';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import * as vscode from 'vscode';

const EXTENSION_ID = 'nolindnaidoo.ids-le';

function file(name: string, content: string): vscode.Uri {
	const path = join(mkdtempSync(join(tmpdir(), 'ids-le-it-')), name);
	writeFileSync(path, content);
	return vscode.Uri.file(path);
}

async function extractFrom(uri: vscode.Uri): Promise<string> {
	await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(uri));
	await vscode.commands.executeCommand('ids-le.extract');
	const report = vscode.workspace.textDocuments
		.filter((doc) => doc.languageId === 'markdown' && doc.getText().includes('IDs-LE report'))
		.find((doc) => doc.getText().includes(vscode.workspace.asRelativePath(uri, false)));
	assert.ok(report, 'no report document found');
	return report.getText();
}

describe('IDs-LE integration', function () {
	this.timeout(30_000);

	it('activates', async () => {
		const extension = vscode.extensions.getExtension(EXTENSION_ID);
		assert.ok(extension, `extension ${EXTENSION_ID} not found`);
		await extension.activate();
		assert.strictEqual(extension.isActive, true);
	});

	it('registers every declared command', async () => {
		const extension = vscode.extensions.getExtension(EXTENSION_ID);
		await extension?.activate();
		const commands = await vscode.commands.getCommands(true);
		for (const id of [
			'ids-le.extract',
			'ids-le.scanWorkspace',
			'ids-le.scanFolder',
			'ids-le.openSettings',
			'ids-le.help',
		]) {
			assert.ok(commands.includes(id), `missing command: ${id}`);
		}
	});

	it('names each identifier with its key path and decoded time, and refuses the digest by name', async () => {
		const text = await extractFrom(
			file(
				'config.json',
				JSON.stringify({ service: { requestId: '019ff344-cc00-7abc-8def-0123456789ab' }, digest: '5d41402abc4b2a76b9719d911017c592' }, null, 2),
			),
		);
		assert.ok(text.includes('key `service.requestId` · v7 · 2026-08-12T00:00:00.000Z'), text);
		assert.ok(text.includes('ambiguous_kind: 32 hex digits'), text);
	});

	it('reads a .env.local as dotenv, so its key names the ObjectId', async () => {
		const text = await extractFrom(file('.env.local', 'USER_ID=6a7bb780a1b2c3d4e5f60718\n'));
		assert.ok(text.includes('## objectid (1)'), text);
	});

	it('offers its MCP server to agent mode', async () => {
		// The registration itself is only observable in a real host, which
		// scripts/e2e-vsix.js covers against the installed VSIX.
		const extension = vscode.extensions.getExtension(EXTENSION_ID);
		await extension?.activate();
		assert.strictEqual(
			typeof vscode.lm.registerMcpServerDefinitionProvider,
			'function',
			'this VS Code build predates the MCP provider API',
		);
		const providers = extension?.packageJSON.contributes.mcpServerDefinitionProviders as {
			id: string;
			label: string;
		}[];
		assert.deepStrictEqual(
			providers.map((p) => p.id),
			['ids-le'],
		);
	});
	it('scans a folder from disk: a section per file, excludes honoured, binaries left unread, refusals in Problems', async () => {
		const root = mkdtempSync(join(tmpdir(), 'ids-le-scan-'));
		mkdirSync(join(root, 'api'));
		mkdirSync(join(root, 'node_modules'));
		const uuid = 'f47ac10b-58cc-4372-a567-0e02b2c3d479';
		const bad = 'f47ac10b-58cc-4372-1567-0e02b2c3d479';
		writeFileSync(join(root, 'api', 'a.json'), JSON.stringify({ id: uuid }));
		writeFileSync(join(root, 'api', 'b.txt'), `first ${uuid}\nthen ${bad}\n`);
		writeFileSync(join(root, 'node_modules', 'dep.json'), JSON.stringify({ id: uuid }));
		writeFileSync(join(root, 'logo.bin'), Buffer.from([0x89, 0x50, 0x00, 0x47]));
		writeFileSync(join(root, 'empty.md'), 'nothing here');

		writeFileSync(join(root, '.gitignore'), 'generated/\n');
		mkdirSync(join(root, 'generated'));
		writeFileSync(join(root, 'generated', 'g.json'), JSON.stringify({ id: uuid }));
		const settings = vscode.workspace.getConfiguration('ids-le');
		await settings.update('workspace.scanProblemsEnabled', true, vscode.ConfigurationTarget.Global);

		// As the Explorer calls it: with the folder that was clicked.
		await vscode.commands.executeCommand('ids-le.scanFolder', vscode.Uri.file(root));
		await settings.update('workspace.scanProblemsEnabled', undefined, vscode.ConfigurationTarget.Global);

		// This scan's report, whatever other reports the session has open.
		const report = vscode.workspace.textDocuments.find(
			(doc) => doc.languageId === 'markdown' && doc.getText().includes('ids-le-scan-'),
		);
		assert.ok(report, 'no workspace report was opened');
		const text = report.getText();
		// The .gitignore itself is read, and what it names is not.
		assert.match(text, /4 file\(s\) read · 2 named, 1 could not be named/);
		assert.ok(!text.includes('generated'), 'a file ignored by .gitignore was read');
		assert.match(text, /\| `api\/b\.txt` \| 1 \| 1 \|/);
		assert.deepStrictEqual(text.match(/^## .*$/gm), ['## `api/a.json` · json (1)', '## `api/b.txt` · text (1)']);
		assert.ok(!text.includes('node_modules'), 'an excluded folder was read');
		// `.bin` is on the list of extensions that are not text, so the file is
		// never opened, and the report says which filters were on.
		assert.match(text, /> Not read: dependency folders, build output, caches and lockfiles; images, fonts, archives and other binary files; 1 file\(s\) ignored by \.gitignore\./);

		const problems = vscode.languages
			.getDiagnostics()
			.filter(([, list]) => list.some((d) => d.source === 'ids-le'));
		assert.strictEqual(problems.length, 1, 'expected problems for one file');
		const [uri, list] = problems[0] as [vscode.Uri, vscode.Diagnostic[]];
		assert.ok(uri.path.endsWith('/api/b.txt'));
		assert.strictEqual(list.length, 1);
		assert.strictEqual(list[0]?.severity, vscode.DiagnosticSeverity.Warning);
		assert.strictEqual(list[0]?.range.start.line, 1);
		assert.strictEqual(list[0]?.range.start.character, 5);
	});
});
