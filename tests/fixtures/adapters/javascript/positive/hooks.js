// Exported React hooks — adapter emits BOTH a function/binding symbol
// AND a separate "hook" symbol for the same line (see NOTES gap).
export function useAuth() {
  return useContext(AuthContext);
}

export const useTheme = () => {
  return useContext(ThemeContext);
};
