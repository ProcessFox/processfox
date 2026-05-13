export interface Settings {
  defaultProvider: string | null;
  defaultModels: Record<string, string>;
  firstRunDone: boolean;
  customOpenAiBaseUrl: string | null;
  locale: string | null;
}
