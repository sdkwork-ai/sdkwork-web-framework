/**
 * Thin i18n registry for the web-framework PC console.
 * Only aggregates and re-exports authored fragments; no message copy lives here.
 */
import {
  consoleMessages,
  consoleTabLabels,
  type ConsoleMessageKey,
} from "./zh-CN/webFramework/console/messages";

export const messages = consoleMessages;
export const tabLabels = consoleTabLabels;

export type MessageKey = ConsoleMessageKey;