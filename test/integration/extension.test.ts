import * as assert from 'node:assert';
import { mkdtempSync, writeFileSync } from 'node:fs';
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
		for (const id of ['ids-le.extract', 'ids-le.openSettings', 'ids-le.help']) {
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
});
