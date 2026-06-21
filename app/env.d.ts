declare namespace NodeJS {
  interface ProcessEnv {
    /**
     * Expo 会在构建时内联 EXPO_PUBLIC_ 前缀变量。
     * 这里显式声明该公开配置，既满足点号访问的构建要求，也保留
     * noPropertyAccessFromIndexSignature 对其它环境变量的约束。
     */
    EXPO_PUBLIC_API_BASE_URL?: string;
  }
}
