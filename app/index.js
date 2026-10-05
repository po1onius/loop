import { getApp } from '@react-native-firebase/app';
import { getMessaging, setBackgroundMessageHandler } from '@react-native-firebase/messaging';

// The OS displays notification payloads. History is synchronized after foregrounding.
setBackgroundMessageHandler(getMessaging(getApp()), async () => {});
require('expo-router/entry');
