import { useState } from "react";
import {
  KeyboardAvoidingView,
  Platform,
  Pressable,
  StyleSheet,
  TextInput,
} from "react-native";
import { router } from "expo-router";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";

export default function LoginScreen() {
  const [account, setAccount] = useState("");
  const [password, setPassword] = useState("");

  const handleLogin = () => {
    if (!account.trim() || !password) {
      return;
    }
    router.replace("/(tabs)");
  };

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        style={styles.flex}
      >
        <ThemedView style={styles.container}>
          <ThemedView style={styles.card} lightColor="#F5F8FC" darkColor="#1E252C">
            <ThemedText type="title" style={styles.title}>
              登录
            </ThemedText>
            <ThemedText style={styles.subtitle}>
              输入账号和密码以继续使用应用
            </ThemedText>

            <TextInput
              style={styles.input}
              placeholder="手机号 / 邮箱"
              placeholderTextColor="#8A94A6"
              autoCapitalize="none"
              value={account}
              onChangeText={setAccount}
            />
            <TextInput
              style={styles.input}
              placeholder="密码"
              placeholderTextColor="#8A94A6"
              secureTextEntry
              value={password}
              onChangeText={setPassword}
            />

            <Pressable
              style={[
                styles.button,
                !account.trim() || !password ? styles.buttonDisabled : undefined,
              ]}
              onPress={handleLogin}
              disabled={!account.trim() || !password}
            >
              <ThemedText style={styles.buttonText}>登录</ThemedText>
            </Pressable>
          </ThemedView>
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
  },
  flex: {
    flex: 1,
  },
  container: {
    flex: 1,
    justifyContent: "center",
    paddingHorizontal: 20,
  },
  card: {
    borderRadius: 18,
    paddingHorizontal: 18,
    paddingVertical: 22,
    gap: 12,
  },
  title: {
    fontSize: 30,
  },
  subtitle: {
    marginBottom: 8,
    color: "#687076",
  },
  input: {
    height: 48,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: "#CDD3DD",
    paddingHorizontal: 14,
    fontSize: 16,
    color: "#11181C",
    backgroundColor: "#FFFFFF",
  },
  button: {
    marginTop: 8,
    height: 48,
    borderRadius: 10,
    backgroundColor: "#0A7EA4",
    alignItems: "center",
    justifyContent: "center",
  },
  buttonDisabled: {
    opacity: 0.55,
  },
  buttonText: {
    color: "#FFFFFF",
    fontWeight: "600",
  },
});
