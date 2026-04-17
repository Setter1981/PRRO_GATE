using System;
using System.Diagnostics;
using Google.Protobuf;
using Google.Protobuf.Reflection;

namespace Com.Programika.Rro.Ws.Chk;

public sealed class CheckRequest : IMessage<CheckRequest>, IMessage, IEquatable<CheckRequest>, IDeepCloneable<CheckRequest>
{
	private static readonly MessageParser<CheckRequest> _parser = new MessageParser<CheckRequest>(() => new CheckRequest());

	private UnknownFieldSet _unknownFields;

	public const int RroFnSignFieldNumber = 3;

	private ByteString rroFnSign_ = ByteString.Empty;

	[DebuggerNonUserCode]
	public static MessageParser<CheckRequest> Parser => _parser;

	[DebuggerNonUserCode]
	public static MessageDescriptor Descriptor => GreetReflection.Descriptor.MessageTypes[1];

	[DebuggerNonUserCode]
	MessageDescriptor IMessage.Descriptor => Descriptor;

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
	public CheckRequest()
	{
	}

	[DebuggerNonUserCode]
	public CheckRequest(CheckRequest other)
		: this()
	{
		rroFnSign_ = other.rroFnSign_;
		_unknownFields = UnknownFieldSet.Clone(other._unknownFields);
	}

	[DebuggerNonUserCode]
	public CheckRequest Clone()
	{
		return new CheckRequest(this);
	}

	[DebuggerNonUserCode]
	public override bool Equals(object other)
	{
		return Equals(other as CheckRequest);
	}

	[DebuggerNonUserCode]
	public bool Equals(CheckRequest other)
	{
		if (other == null)
		{
			return false;
		}
		if (other == this)
		{
			return true;
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
		if (RroFnSign.Length != 0)
		{
			output.WriteRawTag(26);
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
	public void MergeFrom(CheckRequest other)
	{
		if (other != null)
		{
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
			if (num != 26)
			{
				_unknownFields = UnknownFieldSet.MergeFieldFrom(_unknownFields, input);
			}
			else
			{
				RroFnSign = input.ReadBytes();
			}
		}
	}
}
