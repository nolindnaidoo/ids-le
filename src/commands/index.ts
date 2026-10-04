import * as vscode from 'vscode';
import type { Telemetry } from '../telemetry/telemetry';
import type { Notifier } from '../ui/notifier';
import type { StatusBar } from '../ui/statusBar';
import { extractFromActiveDocument } from './extract';

export interface CommandDependencies {
	notifier: Notifier;
	statusBar: StatusBar;
	telemetry: Telemetry;
}

export function registerCommands(
	context: vscode.ExtensionContext,
	deps: CommandDependencies,
): void {
	context.subscriptions.push(
		vscode.commands.registerCommand('ids-le.extract', async () =>
			extractFromActiveDocument(deps),
		),
	);
}
