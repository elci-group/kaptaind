export function useTheme(): string {
    return 'light';
}

export function useAuth(): null {
    return null;
} // trailing comment disables hook kind

export const useFlag = (): boolean => true;
