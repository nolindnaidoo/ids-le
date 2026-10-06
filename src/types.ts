import type { Kind } from './extract';

export type NotificationLevel = 'all' | 'important' | 'silent';

/** `all`, or one kind to name: refusals are always reported either way. */
export type KindSetting = 'all' | Kind;

/** The extension's settings, read once per command and frozen. */
export interface Configuration {
	/** Whether the copy on the clipboard carries positions, whatever the report shows. */
	readonly clipboardIncludesPositions: boolean;
	readonly copyToClipboardEnabled: boolean;
	readonly kind: KindSetting;
	readonly notificationsLevel: NotificationLevel;
	readonly openResultsSideBySide: boolean;
	readonly safetyEnabled: boolean;
	readonly safetyFileSizeWarnBytes: number;
	/** Whether the report gives the line and column of each identifier. */
	readonly showPositions: boolean;
	readonly statusBarEnabled: boolean;
	readonly telemetryEnabled: boolean;
	/** List each run that could not be named in a folder scan, not only how many. */
	readonly workspaceScanIncludeRefusals: boolean;
	readonly workspaceScanExcludes: readonly string[];
	/** Publish a folder scan's refusals to the Problems panel. */
	readonly workspaceScanProblemsEnabled: boolean;
	readonly workspaceScanRespectGitignore: boolean;
	readonly workspaceScanMaxFiles: number;
	/** The most identifiers one folder scan reports before it stops reading. */
	readonly workspaceScanMaxResults: number;
	readonly workspaceScanPatterns: readonly string[];
}
