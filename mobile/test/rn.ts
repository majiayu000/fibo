// `react-native` for tests. bun keeps a mocked module's export names for the whole run, across
// files: a later mock.module cannot add a name an earlier file left out, so which test fails would
// depend on the order files run in. Every file starts from these names and changes what it needs.
const unset = (name: string) => () => {
  throw new Error(`test did not mock react-native ${name}`);
};

export const rn = (over: Record<string, unknown> = {}) => ({
  Platform: { OS: 'web' },
  AppState: { addEventListener: () => ({ remove() {} }) },
  Alert: { alert: unset('Alert.alert') },
  ...over,
});
