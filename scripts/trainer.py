#!/usr/bin/python3
import torch
from torch import nn

import pytorch_lightning as pl
import numpy as np

import struct, sys, subprocess

NUM_FEATURES = 2 * 6 * 64
LAYER_1 = 32
BUCKETS = 16
WEIGHT_SCALE = 64
ACTIVATION_RANGE = 127
MIN = -128 / WEIGHT_SCALE
MAX = 127 / WEIGHT_SCALE

class Nnue(pl.LightningModule):
    def __init__(self):
        super(Nnue, self).__init__()

        self.features = nn.Linear(NUM_FEATURES, LAYER_1)
        self.layer1 = nn.Linear(2 * LAYER_1, 1 * BUCKETS)

        self.idx_cache = None

    def forward(self, features, buckets):
        if self.idx_cache is None or self.idx_cache.shape[0] != features[0].shape[0]:
            self.idx_cache = torch.arange(0, features[0].shape[0] * BUCKETS, BUCKETS)
        indices = buckets.flatten() + self.idx_cache

        acc = torch.cat([self.features(features[0]), self.features(features[1])], dim=1)
        l1_input = torch.clamp(acc, 0.0, 1.0)
        l1_out = self.layer1(l1_input)

        l1_out = l1_out.view(-1, 1)[indices]

        return l1_out

    def training_step(self, batch, batch_idx):
        features, buckets, target = batch
        value = torch.sigmoid(self(features, buckets))
        return torch.nn.functional.mse_loss(value, target)

    def optimizer_step(self, *args, **kwargs):
        super().optimizer_step(*args, **kwargs)
        for p in self.layer1.parameters():
            p.data = p.data.clamp(MIN, MAX)

    def configure_optimizers(self):
        return torch.optim.Adam(self.parameters())

#    def training_epoch_end(self, outputs):
#        self.export()
#        out = subprocess.run(
#            ["cargo", "run", "bench"],
#            stdout=subprocess.PIPE,
#            stderr=subprocess.DEVNULL,
#            text=True
#        ).stdout
#        nodes = float(out.split()[0])
#        print(f"bench: {nodes}")
#        self.log("bench", nodes)

    def export(nnue):
        def save_tensor(file, tensor, scale):
            file.write("[")
            for i in range(tensor.shape[0]):
                if len(tensor.shape) == 1:
                    file.write(f"{round(tensor[i] * scale)},")
                else:
                    save_tensor(file, tensor[i], scale)
                    file.write(",")
            file.write("]")

        state = nnue.state_dict()

        with open("frozenight/model.rs", "w") as file:
            file.write("Nnue {")
            file.write("input_layer:")
            save_tensor(file, state["features.weight"].cpu().numpy().transpose(), ACTIVATION_RANGE)
            file.write(",input_layer_bias:")
            save_tensor(file, state["features.bias"].cpu().numpy(), ACTIVATION_RANGE)
            file.write(",hidden_layer:")
            save_tensor(file, state["layer1.weight"].cpu().numpy(), WEIGHT_SCALE)
            file.write(",hidden_layer_bias:")
            save_tensor(file, state['layer1.bias'].cpu().numpy(), ACTIVATION_RANGE * WEIGHT_SCALE)
            file.write("}")

class PositionSet(torch.utils.data.Dataset):
    def __init__(self, data: bytes):
        self.data = data

    def __len__(self) -> int:
        return len(self.data) // 132

    def __getitem__(self, index: int):
        content = struct.unpack("<" + "H" * 64 + "hBB", self.data[index*132:index*132+132])
        stm = np.zeros(NUM_FEATURES, dtype=np.float32)
        for i in range(32):
            if content[i] == 65535: break
            stm[content[i]] = 1
        sntm = np.zeros(NUM_FEATURES, dtype=np.float32)
        for i in range(32):
            if content[32 + i] == 65535: break
            sntm[content[32 + i]] = 1
        value = torch.sigmoid(torch.tensor([content[64] / ACTIVATION_RANGE / WEIGHT_SCALE * 8]))
        outcome = content[65] / 2
        bucket = min(content[66] * BUCKETS // 76, BUCKETS - 1)
        t = 0.9
        target = value * t + outcome * (1 - t)
        return [torch.as_tensor(stm), torch.as_tensor(sntm)], bucket, torch.tensor([target])

if __name__ != "__main__":
    pass
elif sys.argv[1] == "train":
    with open(sys.argv[2], "rb") as f:
        dataset = PositionSet(f.read())
    dataloader = torch.utils.data.DataLoader(dataset, batch_size=1<<13, shuffle=True, num_workers=32)

    nnue = Nnue()
    trainer = pl.Trainer(callbacks=pl.callbacks.ModelCheckpoint(save_top_k=-1), max_epochs=40)
    trainer.fit(nnue, train_dataloaders=dataloader)
elif sys.argv[1] == "dump":
    Nnue.load_from_checkpoint(sys.argv[2]).export()
