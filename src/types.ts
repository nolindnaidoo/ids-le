import type { Kind } from './extract';

export type NotificationLevel = 'all' | 'important' | 'silent';

/** `all`, or one kind to name: refusals are always reported either way. */
export type KindSetting = 'all' | Kind;

/** The extension's settings, read once per command and frozen. */
export interface Configuration {
	readonly copyToClipboardEnabled: boolean;
	readonly kind: KindSetting;
	readonly notificationsLevel: NotificationLevel;
	readonly openResultsSideBySide: boolean;
	readonly safetyEnabled: boolean;
	readonly safetyFileSizeWarnBytes: number;
	readonly statusBarEnabled: boolean;
	readonly telemetryEnabled: boolean;
}
