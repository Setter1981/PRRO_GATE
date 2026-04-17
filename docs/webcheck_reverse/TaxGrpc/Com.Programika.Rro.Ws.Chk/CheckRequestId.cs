using System;
using System.Diagnostics;
using Google.Protobuf;
using Google.Protobuf.Reflection;

namespace Com.Programika.Rro.Ws.Chk;

public sealed class CheckRequestId : IMessage<CheckRequestId>, IMessage, IEquatable<CheckRequestId>, IDeepCloneable<CheckRequestId>
{
	private static readonly MessageParser<CheckRequestId> _parser = new MessageParser<CheckRequestId>(() => new CheckRequestId());

	private UnknownFieldSet _unknownFields;

	public const int IdFieldNumber = 1;

	private string id_ = "";

	public const int RroFnSignFieldNumber = 2;

	private ByteString rroFnSign_ = ByteString.Empty;

	[DebuggerNonUserCode]
	public static MessageParser<CheckRequestId> Parser => _parser;

	[DebuggerNonUserCode]
	public static MessageDescriptor Descriptor => GreetReflection.Descriptor.MessageTypes[2];

	[DebuggerNonUserCode]
	MessageDescriptor IMessage.Descriptor => Descriptor;

	[DebuggerNonUserCode]
	public string Id
	{
		get
		{
			return id_;
		}
		set
		{
			id_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public ByteString RroFnSign
	{
		get
		{
			return rroFnSign_;
		}
		set
		{
			rroFnSign_ = ProtoPreconditions.CheckNotNull(value, "value");
		}
	}

	[DebuggerNonUserCode]
	public CheckRequestId()
	{
	}

	[DebuggerNonUserCode]
	public CheckRequestId(CheckRequestId other)
		: this()
	{
		id_ = other.id_;
		rroFnSign_ = other.rroFnSign_;
		_unknownFields = UnknownFieldSet.Clone(other._unknownFields);
	}

	[DebuggerNonUserCode]
	public CheckRequestId Clone()
	{
		return new CheckRequestId(this);
	}

	[DebuggerNonUserCode]
	public override bool Equals(object other)
	{
		return Equals(other as CheckRequestId);
	}

	[DebuggerNonUserCode]
	public bool Equals(CheckRequestId other)
	{
		if (other == null)
		{
			return false;
		}
		if (other == this)
		{
			return true;
		}
		if (Id != other.Id)
		{
			return false;
		}
		if (RroFnSign != other.RroFnSign)
		{
			return false;
		}
		return object.Equals(_unknownFields, other._unknownFields);
	}

	[DebuggerNonUserCode]
	public override int GetHashCode()
	{
		int num = 1;
		if (Id.Length != 0)
		{
			num ^= Id.GetHashCode();
		}
		if (RroFnSign.Length != 0)
		{
			num ^= RroFnSign.GetHashCode();
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
		if (Id.Length != 0)
		{
			output.WriteRawTag(10);
			output.WriteString(Id);
		}
		if (RroFnSign.Length != 0)
		{
			output.WriteRawTag(18);
			output.WriteBytes(RroFnSign);
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
		if (Id.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeStringSize(Id);
		}
		if (RroFnSign.Length != 0)
		{
			num += 1 + CodedOutputStream.ComputeBytesSize(RroFnSign);
		}
		if (_unknownFields != null)
		{
			num += _unknownFields.CalculateSize();
		}
		return num;
	}

	[DebuggerNonUserCode]
	public void MergeFrom(CheckRequestId other)
	{
		if (other != null)
		{
			if (other.Id.Length != 0)
			{
				Id = other.Id;
			}
			if (other.RroFnSign.Length != 0)
			{
				RroFnSign = other.RroFnSign;
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
				Id = input.ReadString();
				break;
			case 18u:
				RroFnSign = input.ReadBytes();
				break;
			}
		}
	}
}
