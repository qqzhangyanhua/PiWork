import { useEffect } from "react";

import { useWorkStoreContext } from "./WorkStoreProvider";

export const useWorkEvents = () => {
  const { client, store } = useWorkStoreContext();

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void client
      .listenToWorkEvents((event) => {
        store.getState().applyEvent(event);
      })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      })
      .catch((error: unknown) => {
        if (!disposed) {
          store.setState({
            error: {
              code: "event_subscription_failed",
              message: error instanceof Error ? error.message : String(error),
            },
          });
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [client, store]);
};
