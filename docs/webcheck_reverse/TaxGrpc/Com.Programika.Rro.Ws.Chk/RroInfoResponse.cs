using System;
using System.Diagnostics;
using Google.Protobuf;
using Google.Protobuf.Collections;
using Google.Protobuf.Reflection;

namespace Com.Programika.Rro.Ws.Chk;

public sealed class RroInfoResponse : IMessage<RroInfoResponse>, IMessage, IEquatable<RroInfoResponse>, IDeepCloneable<RroInfoResponse>
{
	[DebuggerNonUserCode]
	public static class Types
	{
		public enum Status
		{
			[OriginalName("UNKNOWN")]
			Unknown = 0,
			[OriginalName("OK")]
			Ok = 1,
			[OriginalName("ERROR_VEREFY")]
			ErrorVerefy = -1,
			[OriginalName("ERROR_CHECK")]
			ErrorCheck = -2,
			[OriginalName("ERROR_UNKNOWN")]
			ErrorUnknown = -4,
			[OriginalName("ERROR_NOT_REGISTERED_RRO")]
			ErrorNotRegisteredRro = -13,
			[OriginalName("ERROR_NOT_REGISTERED_SIGNER")]
			ErrorNotRegisteredSigner = -14
		}

		public sealed class Operator : IMessage<Operator>, IMessage, IEquatable<Operator>, IDeepCloneable<Operator>
		{
			private static readonly MessageParser<Operator> _parser = new MessageParser<Operator>(() => new Operator());

			private UnknownFieldSet _unknownFields;

			public const int SerialFieldNumber = 1;

			private string serial_ = "";

			public const int StatusFieldNumber = 2;

			private int status_;

			public const int SeniorFieldNumber = 3;

			private bool senior_;

			public const int IsnameFieldNumber = 4;

			private string isname_ = "";

			[DebuggerNonUserCode]
			public static MessageParser<Operator> Parser => _parser;

			[DebuggerNonUserCode]
			public static MessageDescriptor Descriptor => RroInfoResponse.Descriptor.NestedTypes[0];

			[DebuggerNonUserCode]
			MessageDescriptor IMessage.Descriptor => Descriptor;

			[DebuggerNonUserCode]
			public string Serial
			{
				get
				{
					return serial_;
				}
				set
				{
					serial_ = ProtoPreconditions.CheckNotNull(value, "value");
				}
			}

			[DebuggerNonUserCode]
			public int Status
			{
				get
				{
					return status_;
				}
				set
				{
					status_ = value;
				}
			}

			[DebuggerNonUserCode]
			public bool Senior
			{
				get
				{
					return senior_;
				}
				set
				{
					senior_ = value;
				}
			}

			[DebuggerNonUserCode]
			public string Isname
			{
				get
				{
					return isname_;
				}
				set
				{
					isname_ = ProtoPreconditions.CheckNotNull(value, "value");
				}
			}

			[DebuggerNonUserCode]
			public Operator()
			{
			}

			[DebuggerNonUserCode]
			public Operator(Operator other)
				: this()
			{
				serial_ = other.serial_;
				status_ = other.status_;
				senior_ = other.senior_;
				isname_ = other.isname_;
				_unknownFields = UnknownFieldSet.Clone(other._unknownFields);
			}

			[DebuggerNonUserCode]
			public Operator Clone()
			{
				return new Operator(this);
			}

			[DebuggerNonUserCode]
			public override bool Equals(object other)
			{
				return Equals(other as Operator);
			}

			[DebuggerNonUserCode]
			public bool Equals(Operator other)
			{
				if (other == null)
				{
					return false;
				}
				if (other == this)
				{
					return true;
				}
				if (Serial != other.Serial)
				{
					return false;
				}
				if (Status != other.Status)
				{
					return false;
				}
				if (Senior != other.Senior)
				{
					return false;
				}
				if (Isname != other.Isname)
				{
					return false;
				}
				return object.Equals(_unknownFields, other._unknownFields);
			}

			[DebuggerNonUserCode]
			public override int GetHashCode()
			{
				int num = 1;
				if (Serial.Length != 0)
				{
					num ^= Serial.GetHashCode();
				}
				if (Status != 0)
				{
					num ^= Status.GetHashCode();
				}
				if (Senior)
				{
					num ^= Senior.GetHashCode();
				}
				if (Isname.Length != 0)
				{
					num ^= Isname.GetHashCode();
				}
				if (_unknownFields != null)
				{
					num ^= _unknownFields.GetHashCode();
				}
				return num;
			}

			[DebuggerNonUserCode]
			public override string ToString()
			{
				return JsonFormatter.ToDiagnosticString(this);
			}

			[DebuggerNonUserCode]
			public void WriteTo(CodedOutputStream output)
			{
				if (Serial.Length != 0)
				{
					output.WriteRawTag(10);
					output.WriteString(Serial);
				}
				if (Status != 0)
				{
					output.WriteRawTag(16);
					output.WriteInt32(Status);
				}
				if (Senior)
				{
					output.WriteRawTag(24);
					output.WriteBool(Senior);
				}
				if (Isname.Length != 0)
				{
					output.WriteRawTag(34);
					output.WriteString(Isname);
				}
				if (_unknownFields != null)
				{
					_unknownFields.WriteTo(output);
				}
			}

			[DebuggerNonUserCode]
			public int CalculateSize()
			{
				int num = 0;
				if (Serial.Length != 0)
				{
					num += 1 + CodedOutputStream.ComputeStringSize(Serial);
				}
				if (Status != 0)
				{
					num += 1 + CodedOutputStream.ComputeInt32Size(Status);
				}
				if (Senior)
				{
					num += 2;
				}
				if (Isname.Length != 0)
				{
					num += 1 + CodedOutputStream.ComputeStringSize(Isname);
				}
				if (_unknownFields != null)
				{
					num += _unknownFields.CalculateSize();
				}
				return num;
			}

			[DebuggerNonUserCode]
			public void MergeFrom(Operator other)
			{
				if (other != null)
				{
					if (other.Serial.Length != 0)
					{
						Serial = other.Serial;
					}
					if (other.Status != 0)
					{
						Status = other.Status;
					}
					if (other.Senior)
					{
						Senior = other.Senior;
					}
					if (other.Isname.Length != 0)
					{
						Isname = other.Isname;
					}
					_unknownFields = UnknownFieldSet.MergeFrom(_unknownFields, other._unknownFields);
				}
			}

			[DebuggerNonUserCode]
			public void MergeFrom(CodedInputStream input)
			{
				uint num;
				while ((num = input.ReadTag()) != 0)
				{
					switch (num)
					{
					default:
						_unknownFields = UnknownFieldSet.MergeFieldFrom(_unknownFields, input);
						break;
					case 10u:
						Serial = input.ReadString();
						break;
					case 16u:
						Status = input.ReadInt32();
						break;
					case 24u:
						Senior = input.ReadBool();
						break;
					case 34u:
						Isname = input.ReadString();
						break;
					}
				}
			}
		}
	}

	private static readonly MessageParser<RroInfoResponse> _parser = new MessageParser<RroInfoResponse>(() => new RroInfoResponse());

	private UnknownFieldSet _unknownFields;

	public const int StatusFieldNumber = 1;

	private Types.Status status_;

	public const int StatusRroFieldNumber = 2;

	private int statusRro_;

	public const int OpenShiftFieldNumber = 3;

	private bool openShift_;

	public const int OnlineFieldNumber = 4;

	private bool online_;

	public const int LastSignerFieldNumber = 5;

	private string lastSigner_ = "";

	public const int NameFieldNumber = 6;

	private string name_ = "";

	public const int NameToFieldNumber = 7;

	private string nameTo_ = "";

	public const int AddrFieldNumber = 8;

	private string addr_ = "";

	public const int SingleTaxFieldNumber = 9;

	private bool singleTax_;

	public const int OfflineAllowedFieldNumber = 10;

	private bool offlineAllowed_;

	public const int AddNumFieldNumber = 11;

	private int addNum_;

	public const int PnFieldNumber = 12;

	private string pn_ = "";

	public const int OperatorsFieldNumber = 13;

	private static readonly FieldCodec<Types.Operator> _repeated_operators_codec = FieldCodec.ForMessage(106u, Types.Operator.Parser);

	private readonly RepeatedField<Types.Operator> operators_ = new RepeatedField<Types.Operator>();

	public const int TinsFieldNumber = 14;

	private string tins_ = "";

	public const int LnumFieldNumber = 15;

	private int lnum_;

	public const int NamePayFieldNumber = 16;

	private string namePay_ = "";

	[DebuggerNonUserCode]
	public static MessageParser<RroInfoResponse> Parser => _parser;

	[DebuggerNonUserCode]
	public static MessageDescriptor Descriptor => GreetReflection.Descriptor.MessageTypes[5];

	[DebuggerNonUserCode]
	MessageDescriptor IMessage.Descriptor => Descriptor;

	[DebuggerNonUserCode]
	public Types.Status Status
	{
		get
		{
			return status_;
		}
		set
		{
			status_ = value;
		}
	}

	[DebuggerNonUserCode]
	public int StatusRro
	{
		get
		{
			return statusRro_;
		}
		set
		{
			statusRro_ = value;
		}
	}

	[DebuggerNonUserCode]
	public bool OpenShift
	{
		get
		{
			return openShift_;
		}
		set
		{
			openShift_ = value;
		}
	}

	[DebuggerNonUserCode]
	public bool Online
	{
		get
		{
			return online_;
		}
		set
		{
			online_ = value;
		}
	}

	[DebuggerNonUserCode]
	public string LastSigner
	{
		get
		{
			return lastSigner_;
		}
		set
		{
			lastSigner_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public string Name
	{
		get
		{
			return name_;
		}
		set
		{
			name_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public string NameTo
	{
		get
		{
			return nameTo_;
		}
		set
		{
			nameTo_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public string Addr
	{
		get
		{
			return addr_;
		}
		set
		{
			addr_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public bool SingleTax
	{
		get
		{
			return singleTax_;
		}
		set
		{
			singleTax_ = value;
		}
	}

	[DebuggerNonUserCode]
	public bool OfflineAllowed
	{
		get
		{
			return offlineAllowed_;
		}
		set
		{
			offlineAllowed_ = value;
		}
	}

	[DebuggerNonUserCode]
	public int AddNum
	{
		get
		{
			return addNum_;
		}
		set
		{
			addNum_ = value;
		}
	}

	[DebuggerNonUserCode]
	public string Pn
	{
		get
		{
			return pn_;
		}
		set
		{
			pn_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public RepeatedField<Types.Operator> Operators => operators_;

	[DebuggerNonUserCode]
	public string Tins
	{
		get
		{
			return tins_;
		}
		set
		{
			tins_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public int Lnum
	{
		get
		{
			return lnum_;
		}
		set
		{
			lnum_ = value;
		}
	}

	[DebuggerNonUserCode]
	public string NamePay
	{
		get
		{
			return namePay_;
		}
		set
		{
			namePay_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public RroInfoResponse()
	{
	}

	[DebuggerNonUserCode]
	public RroInfoResponse(RroInfoResponse other)
		: this()
	{
		status_ = other.status_;
		statusRro_ = other.statusRro_;
		openShift_ = other.openShift_;
		online_ = other.online_;
		lastSigner_ = other.lastSigner_;
		name_ = other.name_;
		nameTo_ = other.nameTo_;
		addr_ = other.addr_;
		singleTax_ = other.singleTax_;
		offlineAllowed_ = other.offlineAllowed_;
		addNum_ = other.addNum_;
		pn_ = other.pn_;
		operators_ = other.operators_.Clone();
		tins_ = other.tins_;
		lnum_ = other.lnum_;
		namePay_ = other.namePay_;
		_unknownFields = UnknownFieldSet.Clone(other._unknownFields);
	}

	[DebuggerNonUserCode]
	public RroInfoResponse Clone()
	{
		return new RroInfoResponse(this);
	}

	[DebuggerNonUserCode]
	public override bool Equals(object other)
	{
		return Equals(other as RroInfoResponse);
	}

	[DebuggerNonUserCode]
	public bool Equals(RroInfoResponse other)
	{
		if (other == null)
		{
			return false;
		}
		if (other == this)
		{
			return true;
		}
		if (Status != other.Status)
		{
			return false;
		}
		if (StatusRro != other.StatusRro)
		{
			return false;
		}
		if (OpenShift != other.OpenShift)
		{
			return false;
		}
		if (Online != other.Online)
		{
			return false;
		}
		if (LastSigner != other.LastSigner)
		{
			return false;
		}
		if (Name != other.Name)
		{
			return false;
		}
		if (NameTo != other.NameTo)
		{
			return false;
		}
		if (Addr != other.Addr)
		{
			return false;
		}
		if (SingleTax != other.SingleTax)
		{
			return false;
		}
		if (OfflineAllowed != other.OfflineAllowed)
		{
			return false;
		}
		if (AddNum != other.AddNum)
		{
			return false;
		}
		if (Pn != other.Pn)
		{
			return false;
		}
		if (!operators_.Equals(other.operators_))
		{
			return false;
		}
		if (Tins != other.Tins)
		{
			return false;
		}
		if (Lnum != other.Lnum)
		{
			return false;
		}
		if (NamePay != other.NamePay)
		{
			return false;
		}
		return object.Equals(_unknownFields, other._unknownFields);
	}

	[DebuggerNonUserCode]
	public override int GetHashCode()
	{
		int num = 1;
		if (Status != 0)
		{
			num ^= Status.GetHashCode();
		}
		if (StatusRro != 0)
		{
			num ^= StatusRro.GetHashCode();
		}
		if (OpenShift)
		{
			num ^= OpenShift.GetHashCode();
		}
		if (Online)
		{
			num ^= Online.GetHashCode();
		}
		if (LastSigner.Length != 0)
		{
			num ^= LastSigner.GetHashCode();
		}
		if (Name.Length != 0)
		{
			num ^= Name.GetHashCode();
		}
		if (NameTo.Length != 0)
		{
			num ^= NameTo.GetHashCode();
		}
		if (Addr.Length != 0)
		{
			num ^= Addr.GetHashCode();
		}
		if (SingleTax)
		{
			num ^= SingleTax.GetHashCode();
		}
		if (OfflineAllowed)
		{
			num ^= OfflineAllowed.GetHashCode();
		}
		if (AddNum != 0)
		{
			num ^= AddNum.GetHashCode();
		}
		if (Pn.Length != 0)
		{
			num ^= Pn.GetHashCode();
		}
		num ^= operators_.GetHashCode();
		if (Tins.Length != 0)
		{
			num ^= Tins.GetHashCode();
		}
		if (Lnum != 0)
		{
			num ^= Lnum.GetHashCode();
		}
		if (NamePay.Length != 0)
		{
			num ^= NamePay.GetHashCode();
		}
		if (_unknownFields != null)
		{
			num ^= _unknownFields.GetHashCode();
		}
		return num;
	}

	[DebuggerNonUserCode]
	public override string ToString()
	{
		return JsonFormatter.ToDiagnosticString(this);
	}

	[DebuggerNonUserCode]
	public void WriteTo(CodedOutputStream output)
	{
		if (Status != 0)
		{
			output.WriteRawTag(8);
			output.WriteEnum((int)Status);
		}
		if (StatusRro != 0)
		{
			output.WriteRawTag(16);
			output.WriteInt32(StatusRro);
		}
		if (OpenShift)
		{
			output.WriteRawTag(24);
			output.WriteBool(OpenShift);
		}
		if (Online)
		{
			output.WriteRawTag(32);
			output.WriteBool(Online);
		}
		if (LastSigner.Length != 0)
		{
			output.WriteRawTag(42);
			output.WriteString(LastSigner);
		}
		if (Name.Length != 0)
		{
			output.WriteRawTag(50);
			output.WriteString(Name);
		}
		if (NameTo.Length != 0)
		{
			output.WriteRawTag(58);
			output.WriteString(NameTo);
		}
		if (Addr.Length != 0)
		{
			output.WriteRawTag(66);
			output.WriteString(Addr);
		}
		if (SingleTax)
		{
			output.WriteRawTag(72);
			output.WriteBool(SingleTax);
		}
		if (OfflineAllowed)
		{
			output.WriteRawTag(80);
			output.WriteBool(OfflineAllowed);
		}
		if (AddNum != 0)
		{
			output.WriteRawTag(88);
			output.WriteInt32(AddNum);
		}
		if (Pn.Length != 0)
		{
			output.WriteRawTag(98);
			output.WriteString(Pn);
		}
		operators_.WriteTo(output, _repeated_operators_codec);
		if (Tins.Length != 0)
		{
			output.WriteRawTag(114);
			output.WriteString(Tins);
		}
		if (Lnum != 0)
		{
			output.WriteRawTag(120);
			output.WriteInt32(Lnum);
		}
		if (NamePay.Length != 0)
		{
			output.WriteRawTag(130, 1);
			output.WriteString(NamePay);
		}
		if (_unknownFields != null)
		{
			_unknownFields.WriteTo(output);
		}
	}

	[DebuggerNonUserCode]
	public int CalculateSize()
	{
		int num = 0;
		if (Status != 0)
		{
			num += 1 + CodedOutputStream.ComputeEnumSize((int)Status);
		}
		if (StatusRro != 0)
		{
			num += 1 + CodedOutputStream.ComputeInt32Size(StatusRro);
		}
		if (OpenShift)
		{
			num += 2;
		}
		if (Online)
		{
			num += 2;
		}
		if (LastSigner.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(LastSigner);
		}
		if (Name.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(Name);
		}
		if (NameTo.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(NameTo);
		}
		if (Addr.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(Addr);
		}
		if (SingleTax)
		{
			num += 2;
		}
		if (OfflineAllowed)
		{
			num += 2;
		}
		if (AddNum != 0)
		{
			num += 1 + CodedOutputStream.ComputeInt32Size(AddNum);
		}
		if (Pn.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(Pn);
		}
		num += operators_.CalculateSize(_repeated_operators_codec);
		if (Tins.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(Tins);
		}
		if (Lnum != 0)
		{
			num += 1 + CodedOutputStream.ComputeInt32Size(Lnum);
		}
		if (NamePay.Length != 0)
		{
			num += 2 + CodedOutputStream.ComputeStringSize(NamePay);
		}
		if (_unknownFields != null)
		{
			num += _unknownFields.CalculateSize();
		}
		return num;
	}

	[DebuggerNonUserCode]
	public void MergeFrom(RroInfoResponse other)
	{
		if (other != null)
		{
			if (other.Status != 0)
			{
				Status = other.Status;
			}
			if (other.StatusRro != 0)
			{
				StatusRro = other.StatusRro;
			}
			if (other.OpenShift)
			{
				OpenShift = other.OpenShift;
			}
			if (other.Online)
			{
				Online = other.Online;
			}
			if (other.LastSigner.Length != 0)
			{
				LastSigner = other.LastSigner;
			}
			if (other.Name.Length != 0)
			{
				Name = other.Name;
			}
			if (other.NameTo.Length != 0)
			{
				NameTo = other.NameTo;
			}
			if (other.Addr.Length != 0)
			{
				Addr = other.Addr;
			}
			if (other.SingleTax)
			{
				SingleTax = other.SingleTax;
			}
			if (other.OfflineAllowed)
			{
				OfflineAllowed = other.OfflineAllowed;
			}
			if (other.AddNum != 0)
			{
				AddNum = other.AddNum;
			}
			if (other.Pn.Length != 0)
			{
				Pn = other.Pn;
			}
			operators_.Add(other.operators_);
			if (other.Tins.Length != 0)
			{
				Tins = other.Tins;
			}
			if (other.Lnum != 0)
			{
				Lnum = other.Lnum;
			}
			if (other.NamePay.Length != 0)
			{
				NamePay = other.NamePay;
			}
			_unknownFields = UnknownFieldSet.MergeFrom(_unknownFields, other._unknownFields);
		}
	}

	[DebuggerNonUserCode]
	public void MergeFrom(CodedInputStream input)
	{
		uint num;
		while ((num = input.ReadTag()) != 0)
		{
			switch (num)
			{
			default:
				_unknownFields = UnknownFieldSet.MergeFieldFrom(_unknownFields, input);
				break;
			case 8u:
				Status = (Types.Status)input.ReadEnum();
				break;
			case 16u:
				StatusRro = input.ReadInt32();
				break;
			case 24u:
				OpenShift = input.ReadBool();
				break;
			case 32u:
				Online = input.ReadBool();
				break;
			case 42u:
				LastSigner = input.ReadString();
				break;
			case 50u:
				Name = input.ReadString();
				break;
			case 58u:
				NameTo = input.ReadString();
				break;
			case 66u:
				Addr = input.ReadString();
				break;
			case 72u:
				SingleTax = input.ReadBool();
				break;
			case 80u:
				OfflineAllowed = input.ReadBool();
				break;
			case 88u:
				AddNum = input.ReadInt32();
				break;
			case 98u:
				Pn = input.ReadString();
				break;
			case 106u:
				operators_.AddEntriesFrom(input, _repeated_operators_codec);
				break;
			case 114u:
				Tins = input.ReadString();
				break;
			case 120u:
				Lnum = input.ReadInt32();
				break;
			case 130u:
				NamePay = input.ReadString();
				break;
			}
		}
	}
}
