using System;
using System.Diagnostics;
using Google.Protobuf;
using Google.Protobuf.Reflection;

namespace Com.Programika.Rro.Ws.Chk;

public sealed class Check : IMessage<Check>, IMessage, IEquatable<Check>, IDeepCloneable<Check>
{
	[DebuggerNonUserCode]
	public static class Types
	{
		public enum Type
		{
			[OriginalName("UNKNOWN")]
			Unknown,
			[OriginalName("CHK")]
			Chk,
			[OriginalName("ZREPORT")]
			Zreport,
			[OriginalName("SERVICECHK")]
			Servicechk
		}
	}

	private static readonly MessageParser<Check> _parser = new MessageParser<Check>(() => new Check());

	private UnknownFieldSet _unknownFields;

	public const int RroFnFieldNumber = 1;

	private string rroFn_ = "";

	public const int DateTimeFieldNumber = 2;

	private long dateTime_;

	public const int CheckSignFieldNumber = 3;

	private ByteString checkSign_ = ByteString.Empty;

	public const int LocalNumberFieldNumber = 4;

	private int localNumber_;

	public const int CheckTypeFieldNumber = 5;

	private Types.Type checkType_;

	public const int IdOfflineFieldNumber = 6;

	private string idOffline_ = "";

	public const int IdCancelFieldNumber = 7;

	private string idCancel_ = "";

	[DebuggerNonUserCode]
	public static MessageParser<Check> Parser => _parser;

	[DebuggerNonUserCode]
	public static MessageDescriptor Descriptor => GreetReflection.Descriptor.MessageTypes[0];

	[DebuggerNonUserCode]
	MessageDescriptor IMessage.Descriptor => Descriptor;

	[DebuggerNonUserCode]
	public string RroFn
	{
		get
		{
			return rroFn_;
		}
		set
		{
			rroFn_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public long DateTime
	{
		get
		{
			return dateTime_;
		}
		set
		{
			dateTime_ = value;
		}
	}

	[DebuggerNonUserCode]
	public ByteString CheckSign
	{
		get
		{
			return checkSign_;
		}
		set
		{
			checkSign_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public int LocalNumber
	{
		get
		{
			return localNumber_;
		}
		set
		{
			localNumber_ = value;
		}
	}

	[DebuggerNonUserCode]
	public Types.Type CheckType
	{
		get
		{
			return checkType_;
		}
		set
		{
			checkType_ = value;
		}
	}

	[DebuggerNonUserCode]
	public string IdOffline
	{
		get
		{
			return idOffline_;
		}
		set
		{
			idOffline_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public string IdCancel
	{
		get
		{
			return idCancel_;
		}
		set
		{
			idCancel_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public Check()
	{
	}

	[DebuggerNonUserCode]
	public Check(Check other)
		: this()
	{
		rroFn_ = other.rroFn_;
		dateTime_ = other.dateTime_;
		checkSign_ = other.checkSign_;
		localNumber_ = other.localNumber_;
		checkType_ = other.checkType_;
		idOffline_ = other.idOffline_;
		idCancel_ = other.idCancel_;
		_unknownFields = UnknownFieldSet.Clone(other._unknownFields);
	}

	[DebuggerNonUserCode]
	public Check Clone()
	{
		return new Check(this);
	}

	[DebuggerNonUserCode]
	public override bool Equals(object other)
	{
		return Equals(other as Check);
	}

	[DebuggerNonUserCode]
	public bool Equals(Check other)
	{
		if (other == null)
		{
			return false;
		}
		if (other == this)
		{
			return true;
		}
		if (RroFn != other.RroFn)
		{
			return false;
		}
		if (DateTime != other.DateTime)
		{
			return false;
		}
		if (CheckSign != other.CheckSign)
		{
			return false;
		}
		if (LocalNumber != other.LocalNumber)
		{
			return false;
		}
		if (CheckType != other.CheckType)
		{
			return false;
		}
		if (IdOffline != other.IdOffline)
		{
			return false;
		}
		if (IdCancel != other.IdCancel)
		{
			return false;
		}
		return object.Equals(_unknownFields, other._unknownFields);
	}

	[DebuggerNonUserCode]
	public override int GetHashCode()
	{
		int num = 1;
		if (RroFn.Length != 0)
		{
			num ^= RroFn.GetHashCode();
		}
		if (DateTime != 0L)
		{
			num ^= DateTime.GetHashCode();
		}
		if (CheckSign.Length != 0)
		{
			num ^= CheckSign.GetHashCode();
		}
		if (LocalNumber != 0)
		{
			num ^= LocalNumber.GetHashCode();
		}
		if (CheckType != 0)
		{
			num ^= CheckType.GetHashCode();
		}
		if (IdOffline.Length != 0)
		{
			num ^= IdOffline.GetHashCode();
		}
		if (IdCancel.Length != 0)
		{
			num ^= IdCancel.GetHashCode();
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
		if (RroFn.Length != 0)
		{
			output.WriteRawTag(10);
			output.WriteString(RroFn);
		}
		if (DateTime != 0L)
		{
			output.WriteRawTag(16);
			output.WriteInt64(DateTime);
		}
		if (CheckSign.Length != 0)
		{
			output.WriteRawTag(26);
			output.WriteBytes(CheckSign);
		}
		if (LocalNumber != 0)
		{
			output.WriteRawTag(32);
			output.WriteInt32(LocalNumber);
		}
		if (CheckType != 0)
		{
			output.WriteRawTag(40);
			output.WriteEnum((int)CheckType);
		}
		if (IdOffline.Length != 0)
		{
			output.WriteRawTag(50);
			output.WriteString(IdOffline);
		}
		if (IdCancel.Length != 0)
		{
			output.WriteRawTag(58);
			output.WriteString(IdCancel);
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
		if (RroFn.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(RroFn);
		}
		if (DateTime != 0L)
		{
			num += 1 + CodedOutputStream.ComputeInt64Size(DateTime);
		}
		if (CheckSign.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeBytesSize(CheckSign);
		}
		if (LocalNumber != 0)
		{
			num += 1 + CodedOutputStream.ComputeInt32Size(LocalNumber);
		}
		if (CheckType != 0)
		{
			num += 1 + CodedOutputStream.ComputeEnumSize((int)CheckType);
		}
		if (IdOffline.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(IdOffline);
		}
		if (IdCancel.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(IdCancel);
		}
		if (_unknownFields != null)
		{
			num += _unknownFields.CalculateSize();
		}
		return num;
	}

	[DebuggerNonUserCode]
	public void MergeFrom(Check other)
	{
		if (other != null)
		{
			if (other.RroFn.Length != 0)
			{
				RroFn = other.RroFn;
			}
			if (other.DateTime != 0L)
			{
				DateTime = other.DateTime;
			}
			if (other.CheckSign.Length != 0)
			{
				CheckSign = other.CheckSign;
			}
			if (other.LocalNumber != 0)
			{
				LocalNumber = other.LocalNumber;
			}
			if (other.CheckType != 0)
			{
				CheckType = other.CheckType;
			}
			if (other.IdOffline.Length != 0)
			{
				IdOffline = other.IdOffline;
			}
			if (other.IdCancel.Length != 0)
			{
				IdCancel = other.IdCancel;
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
				RroFn = input.ReadString();
				break;
			case 16u:
				DateTime = input.ReadInt64();
				break;
			case 26u:
				CheckSign = input.ReadBytes();
				break;
			case 32u:
				LocalNumber = input.ReadInt32();
				break;
			case 40u:
				CheckType = (Types.Type)input.ReadEnum();
				break;
			case 50u:
				IdOffline = input.ReadString();
				break;
			case 58u:
				IdCancel = input.ReadString();
				break;
			}
		}
	}
}
