#!/usr/bin/python3
import torch
from torch import nn

import pytorch_lightning as pl
import numpy as np

import struct, sys, json

NUM_FEATURES = 2 * 6 * 64
LAYER_1 = 16
SCALE = 64
MIN = -128 / SCALE
MAX = 127 / SCALE

class Nnue(pl.LightningModule):
    def __init__(self):
        super(Nnue, self).__init__()

        self.features = nn.Linear(NUM_FEATURES, LAYER_1)
        self.layer1 = nn.Linear(2 * LAYER_1, 1)

    def forward(self, features):
        acc = torch.cat([self.features(features[0]), self.features(features[1])], dim=1)
        l1_input = torch.clamp(acc, 0.0, 1.0)
        return self.layer1(l1_input)

    def training_step(self, batch, batch_idx):
        features, target = batch
        value = torch.sigmoid(self(features))
        return torch.nn.functional.binary_cross_entropy(value, target)

    def optimizer_step(self, *args, **kwargs):
        super().optimizer_step(*args, **kwargs)
        for p in self.parameters():
            p.data = p.data.clamp(MIN, MAX)

    def configure_optimizers(self):
        return torch.optim.Adam(self.parameters())

class PositionSet(torch.utils.data.Dataset):
    def __init__(self, data: bytes):
        self.data = data

    def __len__(self) -> int:
        return len(self.data) // 130

    def __getitem__(self, index: int):
        content = struct.unpack("<" + "H" * 65, self.data[index*130:index*130+130])
        stm = np.zeros(NUM_FEATURES, dtype=np.float32)
        for i in range(33):
            if content[i] == 65535: break
            stm[content[i]] = 1
        sntm = np.zeros(NUM_FEATURES, dtype=np.float32)
        for i in range(33):
            if content[32 + i] == 65535: break
            sntm[content[32 + i]] = 1
        outcome = content[64] / 2
        return [torch.as_tensor(stm), torch.as_tensor(sntm)], torch.tensor([outcome])

if __name__ != "__main__":
    pass
elif sys.argv[1] == "train":
    with open("data.bin", "rb") as f:
        dataset = PositionSet(f.read())
    dataloader = torch.utils.data.DataLoader(dataset, batch_size=32, shuffle=True)

    nnue = Nnue()
    trainer = pl.Trainer()
    trainer.fit(nnue, train_dataloaders=dataloader)
elif sys.argv[1] == "dump":
    nnue = Nnue.load_from_checkpoint(sys.argv[2])
    nnue.eval()

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
        save_tensor(file, state["features.weight"].cpu().numpy().transpose(), SCALE)
        file.write(",input_layer_bias:")
        save_tensor(file, state["features.bias"].cpu().numpy(), SCALE)
        file.write(",hidden_layer:")
        save_tensor(file, state["layer1.weight"].cpu().numpy()[0], SCALE)
        file.write(",hidden_layer_bias:")
        file.write(f"{round(state['layer1.bias'].cpu().numpy()[0] * SCALE * SCALE)},")
        file.write("}")
