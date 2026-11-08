// Keeping what you type above the keyboard. Android draws edge to edge since RN 0.88, so the
// window no longer shrinks for the keyboard (adjustResize does nothing): every screen with
// an input lifts itself, on both platforms. Under the keyboard the system bar is hidden, so
// the space kept for it goes too.
import { useEffect, useState } from 'react';
import { Keyboard, KeyboardAvoidingView, Platform, type StyleProp, type ViewStyle } from 'react-native';
import { useBottom } from './kit';

/** A screen or sheet whose bottom edge sits on the keyboard while it is up. */
export function KeyboardSafe({ children, style }: { children: React.ReactNode; style?: StyleProp<ViewStyle> }) {
  return (
    <KeyboardAvoidingView behavior={Platform.OS === 'web' ? undefined : 'padding'} style={[{ flex: 1 }, style]}>
      {children}
    </KeyboardAvoidingView>
  );
}

export function useKeyboardUp() {
  const [up, setUp] = useState(false);
  useEffect(() => {
    // Android sends only the Did events; iOS both, and Will is the earlier one.
    const show = Platform.OS === 'ios' ? 'keyboardWillShow' : 'keyboardDidShow';
    const hide = Platform.OS === 'ios' ? 'keyboardWillHide' : 'keyboardDidHide';
    const a = Keyboard.addListener(show, () => setUp(true));
    const b = Keyboard.addListener(hide, () => setUp(false));
    return () => {
      a.remove();
      b.remove();
    };
  }, []);
  return up;
}

/** `useBottom`, but nothing while the keyboard covers the system bar it keeps clear of. */
export function useBottomOverKeyboard() {
  const bottom = useBottom();
  return useKeyboardUp() ? 0 : bottom;
}
