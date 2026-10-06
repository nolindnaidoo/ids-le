import * as vscode from 'vscode';
import { KIND_NAMES } from '../extract';
import type { Configuration, KindSetting, NotificationLevel } from '../types';

/**
 * The defaults, exported for the parity gate: `config.test.ts` asserts they
 * match every default declared in package.json, which is what stops the two
 * drifting apart.
 */
export const CONFIG_DEFAULTS = Object.freeze({
	clipboardIncludesPositions: true,
	copyToClipboardEnabled: false,
	kind: 'all' as const,
	notificationsLevel: 'silent' as const,
	openResultsSideBySide: true,
	safetyEnabled: true,
	safetyFileSizeWarnBytes: 1_000_000,
	showPositions: true,
	statusBarEnabled: true,
	telemetryEnabled: false,
	workspaceScanAlwaysInclude: Object.freeze([]) as readonly string[],
	workspaceScanExcludes: Object.freeze([]) as readonly string[],
	workspaceScanIncludeRefusals: false,
	workspaceScanMaxFiles: 5000,
	workspaceScanMaxResults: 10000,
	workspaceScanPatterns: Object.freeze(['**/*']) as readonly string[],
	workspaceScanProblemsEnabled: false,
	workspaceScanRespectGitignore: true,
	workspaceScanSkipBinaryFiles: true,
	workspaceScanUseDefaultExcludes: true,
});

export function readConfig(): Configuration {
	const config = vscode.workspace.getConfiguration('ids-le');
	return Object.freeze({
		clipboardIncludesPositions: readBoolean(
			config,
			'clipboardIncludesPositions',
			CONFIG_DEFAULTS.clipboardIncludesPositions,
		),
		copyToClipboardEnabled: readBoolean(
			config,
			'copyToClipboardEnabled',
			CONFIG_DEFAULTS.copyToClipboardEnabled,
		),
		kind: readKind(config),
		notificationsLevel: readNotificationLevel(config),
		openResultsSideBySide: readBoolean(
			config,
			'openResultsSideBySide',
			CONFIG_DEFAULTS.openResultsSideBySide,
		),
		safetyEnabled: readBoolean(
			config,
			'safety.enabled',
			CONFIG_DEFAULTS.safetyEnabled,
		),
		safetyFileSizeWarnBytes: readNumber(
			config,
			'safety.fileSizeWarnBytes',
			CONFIG_DEFAULTS.safetyFileSizeWarnBytes,
			1000,
		),
		showPositions: readBoolean(
			config,
			'showPositions',
			CONFIG_DEFAULTS.showPositions,
		),
		statusBarEnabled: readBoolean(
			config,
			'statusBar.enabled',
			CONFIG_DEFAULTS.statusBarEnabled,
		),
		telemetryEnabled: readBoolean(
			config,
			'telemetryEnabled',
			CONFIG_DEFAULTS.telemetryEnabled,
		),
		workspaceScanAlwaysInclude: readStrings(
			config,
			'workspace.scanAlwaysInclude',
			CONFIG_DEFAULTS.workspaceScanAlwaysInclude,
		),
		workspaceScanExcludes: readStrings(
			config,
			'workspace.scanExcludes',
			CONFIG_DEFAULTS.workspaceScanExcludes,
		),
		workspaceScanSkipBinaryFiles: readBoolean(
			config,
			'workspace.scanSkipBinaryFiles',
			CONFIG_DEFAULTS.workspaceScanSkipBinaryFiles,
		),
		workspaceScanUseDefaultExcludes: readBoolean(
			config,
			'workspace.scanUseDefaultExcludes',
			CONFIG_DEFAULTS.workspaceScanUseDefaultExcludes,
		),
		workspaceScanMaxFiles: readNumber(
			config,
			'workspace.scanMaxFiles',
			CONFIG_DEFAULTS.workspaceScanMaxFiles,
			1,
		),
		workspaceScanMaxResults: readNumber(
			config,
			'workspace.scanMaxResults',
			CONFIG_DEFAULTS.workspaceScanMaxResults,
			1,
		),
		workspaceScanPatterns: readStrings(
			config,
			'workspace.scanPatterns',
			CONFIG_DEFAULTS.workspaceScanPatterns,
		),
		workspaceScanIncludeRefusals: readBoolean(
			config,
			'workspace.scanIncludeRefusals',
			CONFIG_DEFAULTS.workspaceScanIncludeRefusals,
		),
		workspaceScanProblemsEnabled: readBoolean(
			config,
			'workspace.scanProblemsEnabled',
			CONFIG_DEFAULTS.workspaceScanProblemsEnabled,
		),
		workspaceScanRespectGitignore: readBoolean(
			config,
			'workspace.scanRespectGitignore',
			CONFIG_DEFAULTS.workspaceScanRespectGitignore,
		),
	});
}

function readBoolean(
	config: vscode.WorkspaceConfiguration,
	key: string,
	defaultValue: boolean,
): boolean {
	const value = config.get(key, defaultValue);
	return typeof value === 'boolean' ? value : defaultValue;
}

function readNumber(
	config: vscode.WorkspaceConfiguration,
	key: string,
	defaultValue: number,
	minValue: number,
): number {
	const value = Number(config.get(key, defaultValue));
	if (!Number.isFinite(value)) return defaultValue;
	return Math.max(minValue, value);
}

function readStrings(
	config: vscode.WorkspaceConfiguration,
	key: string,
	defaultValue: readonly string[],
): readonly string[] {
	const value = config.get<unknown>(key, defaultValue);
	return Object.freeze(
		Array.isArray(value)
			? value.filter((item): item is string => typeof item === 'string')
			: [...defaultValue],
	);
}

/** An unknown kind falls back to all: a filter must never hide a run silently. */
function readKind(config: vscode.WorkspaceConfiguration): KindSetting {
	const raw = config.get<unknown>('kind', CONFIG_DEFAULTS.kind);
	return KIND_NAMES.find((kind) => kind === raw) ?? CONFIG_DEFAULTS.kind;
}

export function isValidNotificationLevel(v: unknown): v is NotificationLevel {
	return v === 'all' || v === 'important' || v === 'silent';
}

function readNotificationLevel(
	config: vscode.WorkspaceConfiguration,
): NotificationLevel {
	const raw = config.get<string>(
		'notificationsLevel',
		CONFIG_DEFAULTS.notificationsLevel,
	);
	return isValidNotificationLevel(raw)
		? raw
		: CONFIG_DEFAULTS.notificationsLevel;
}
