import * as vscode from 'vscode';
import type { Telemetry } from '../telemetry/telemetry';
import type { Notifier } from '../ui/notifier';
import type { RatingPrompt } from '../ui/ratingPrompt';
import type { StatusBar } from '../ui/statusBar';
import { extractFromActiveDocument } from './extract';
import { scanFolder, scanWorkspace } from './scanWorkspace';

export interface CommandDependencies {
	notifier: Notifier;
	ratingPrompt: RatingPrompt;
	statusBar: StatusBar;
	telemetry: Telemetry;
}

export function registerCommands(
	context: vscode.ExtensionContext,
	deps: CommandDependencies,
): void {
	const diagnostics = vscode.languages.createDiagnosticCollection('ids-le');
	context.subscriptions.push(
		diagnostics,
		vscode.commands.registerCommand('ids-le.extract', async () =>
			extractFromActiveDocument(deps),
		),
		vscode.commands.registerCommand('ids-le.scanWorkspace', async () =>
			scanWorkspace(deps, diagnostics),
		),
		// The Explorer hands over the folder that was clicked. From the
		// palette there is none, and the command asks.
		vscode.commands.registerCommand(
			'ids-le.scanFolder',
			async (picked?: vscode.Uri) => scanFolder(deps, diagnostics, picked),
		),
	);
}
